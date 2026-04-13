use anyhow::{Context, Result};
use rodio::{OutputStream, OutputStreamHandle, Sink};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::info;

use crate::tts::AudioBuffer;

/// Amplitude payload sent to the frontend for waveform visualization.
#[derive(Clone, Serialize)]
pub struct WaveformPayload {
    /// 8 normalized amplitude values (0.0–1.0) representing waveform bars.
    pub amplitudes: Vec<f32>,
}

const WAVEFORM_BARS: usize = 8;

/// Manages the audio output stream for TTS playback.
/// Emits `ren://waveform` events during playback for the speaking animation.
pub struct AudioPlayer {
    _stream: OutputStream,
    handle: OutputStreamHandle,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let (_stream, handle) =
            OutputStream::try_default().context("Failed to open audio output device")?;
        info!("Audio player initialized");
        Ok(Self { _stream, handle })
    }

    /// Play a PCM audio buffer and emit waveform events to the frontend.
    /// Blocks (async via spawn_blocking) until playback is complete.
    pub async fn play(&self, app: &AppHandle, buffer: AudioBuffer, sample_rate: u32) -> Result<()> {
        // Emit waveform events before playback starts for UI responsiveness
        let waveform = compute_waveform(&buffer, WAVEFORM_BARS);
        let _ = app.emit("ren://waveform", WaveformPayload { amplitudes: waveform });

        let sink = Sink::try_new(&self.handle).context("Failed to create audio sink")?;

        // Convert f32 buffer to rodio Source
        let source = rodio::buffer::SamplesBuffer::new(1, sample_rate, buffer);
        sink.append(source);

        // Wait for playback to complete
        tokio::task::spawn_blocking(move || {
            sink.sleep_until_end();
        })
        .await
        .context("Playback task panicked")?;

        // Clear waveform when done
        let _ = app.emit(
            "ren://waveform",
            WaveformPayload {
                amplitudes: vec![0.0; WAVEFORM_BARS],
            },
        );

        Ok(())
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

    // Pad if buffer was smaller than expected
    while bars.len() < bar_count {
        bars.push(0.0);
    }

    // Normalize to [0, 1]
    let max = bars.iter().cloned().fold(0.0f32, f32::max);
    if max > 0.0 {
        bars.iter_mut().for_each(|v| *v /= max);
    }

    bars
}
