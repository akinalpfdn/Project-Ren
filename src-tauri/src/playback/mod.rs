use anyhow::{Context, Result};
use rodio::{OutputStream, Sink};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::tts::AudioBuffer;

/// Amplitude payload sent to the frontend for waveform visualization.
#[derive(Clone, Serialize)]
pub struct WaveformPayload {
    /// Normalized amplitude values (0.0–1.0) representing waveform bars.
    pub amplitudes: Vec<f32>,
}

const WAVEFORM_BARS: usize = 8;

/// Playback command sent to the dedicated audio thread.
struct PlayCommand {
    buffer: AudioBuffer,
    sample_rate: u32,
    done: oneshot::Sender<Result<()>>,
}

/// Manages the audio output stream for TTS playback.
///
/// `rodio::OutputStream` is `!Send` (it owns CPAL handles), so the actual
/// stream lives on a dedicated OS thread. `AudioPlayer` holds only the
/// command sender, making it safely shareable across async tasks.
pub struct AudioPlayer {
    cmd_tx: mpsc::UnboundedSender<PlayCommand>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<PlayCommand>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<()>>();

        std::thread::Builder::new()
            .name("ren-audio".into())
            .spawn(move || {
                let (_stream, handle) = match OutputStream::try_default()
                    .context("Failed to open audio output device")
                {
                    Ok(s) => {
                        let _ = ready_tx.send(Ok(()));
                        s
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(anyhow::anyhow!(e.to_string())));
                        return;
                    }
                };

                // One long-lived sink handles every TTS turn. Creating a
                // fresh sink per play() loses the first ~100 ms while
                // rodio re-registers it with the mixer; reusing one sink
                // keeps the stream continuously clocked so appended audio
                // starts on the next mixer tick.
                let play_sink = match Sink::try_new(&handle) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Could not create playback sink: {}", e);
                        return;
                    }
                };

                while let Some(cmd) = cmd_rx.blocking_recv() {
                    let source = rodio::buffer::SamplesBuffer::new(
                        1,
                        cmd.sample_rate,
                        cmd.buffer,
                    );
                    play_sink.append(source);
                    play_sink.sleep_until_end();
                    let _ = cmd.done.send(Ok(()));
                }
            })
            .context("Failed to spawn audio thread")?;

        ready_rx
            .recv()
            .context("Audio thread terminated before ready signal")??;

        info!("Audio player initialized");
        Ok(Self { cmd_tx })
    }

    /// Play a PCM audio buffer and emit waveform events to the frontend.
    /// Awaits playback completion on the dedicated audio thread.
    pub async fn play(
        &self,
        app: &AppHandle,
        buffer: AudioBuffer,
        sample_rate: u32,
    ) -> Result<()> {
        let waveform = compute_waveform(&buffer, WAVEFORM_BARS);
        let _ = app.emit(
            "ren://waveform",
            WaveformPayload { amplitudes: waveform },
        );

        let (done_tx, done_rx) = oneshot::channel();
        self.cmd_tx
            .send(PlayCommand {
                buffer,
                sample_rate,
                done: done_tx,
            })
            .map_err(|_| anyhow::anyhow!("Audio thread is no longer running"))?;

        let result = done_rx
            .await
            .context("Audio thread dropped completion channel")?;

        let _ = app.emit(
            "ren://waveform",
            WaveformPayload {
                amplitudes: vec![0.0; WAVEFORM_BARS],
            },
        );

        if let Err(ref e) = result {
            error!("Playback failed: {}", e);
        }
        result
    }
}

/// Divides the audio buffer into `bar_count` segments and computes RMS per segment.
/// Returns normalized values 0.0–1.0.
fn compute_waveform(buffer: &[f32], bar_count: usize) -> Vec<f32> {
    if buffer.is_empty() {
        return vec![0.0; bar_count];
    }

    let chunk_size = (buffer.len() / bar_count).max(1);
    let mut bars = Vec::with_capacity(bar_count);

    for chunk in buffer.chunks(chunk_size).take(bar_count) {
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        bars.push(rms);
    }

    while bars.len() < bar_count {
        bars.push(0.0);
    }

    let max = bars.iter().cloned().fold(0.0f32, f32::max);
    if max > 0.0 {
        bars.iter_mut().for_each(|v| *v /= max);
    }

    bars
}
