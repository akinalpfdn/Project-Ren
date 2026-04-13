use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing::{debug, info};
use voice_activity_detector::VoiceActivityDetector;

use crate::audio::capture::AudioSamples;
use crate::config::defaults::{AUDIO_SAMPLE_RATE, VAD_SILENCE_THRESHOLD_MS};

/// Events emitted by the VAD processor.
#[derive(Debug)]
pub enum VadEvent {
    /// Speech started — first voiced frame detected.
    SpeechStart,
    /// Speech ended — silence exceeded threshold; payload is the captured buffer.
    SpeechEnd(AudioSamples),
}

/// Runs VAD on incoming audio frames.
/// Emits `SpeechStart` on first voiced frame.
/// Emits `SpeechEnd(buffer)` once silence exceeds `VAD_SILENCE_THRESHOLD_MS`.
pub async fn run(
    mut audio_rx: mpsc::Receiver<AudioSamples>,
    event_tx: mpsc::Sender<VadEvent>,
) {
    let silence_threshold = Duration::from_millis(VAD_SILENCE_THRESHOLD_MS);
    let mut vad = match VoiceActivityDetector::builder()
        .sample_rate(AUDIO_SAMPLE_RATE as i64)
        .chunk_size(480usize) // 30ms at 16kHz
        .build()
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to initialize VAD: {}", e);
            return;
        }
    };

    let mut in_speech = false;
    let mut capture_buffer: AudioSamples = Vec::new();
    let mut last_voiced: Option<Instant> = None;

    info!("VAD processor started");

    while let Some(frame) = audio_rx.recv().await {
        // Score voiced probability for each chunk within the frame
        let voiced = score_frame(&mut vad, &frame);

        if voiced {
            if !in_speech {
                in_speech = true;
                capture_buffer.clear();
                let _ = event_tx.send(VadEvent::SpeechStart).await;
                info!("VAD: speech started");
            }
            capture_buffer.extend_from_slice(&frame);
            last_voiced = Some(Instant::now());
        } else if in_speech {
            // Still capturing during silence — keep buffering until threshold
            capture_buffer.extend_from_slice(&frame);

            if let Some(last) = last_voiced {
                if last.elapsed() >= silence_threshold {
                    in_speech = false;
                    let captured = std::mem::take(&mut capture_buffer);
                    let duration_ms = (captured.len() as u64 * 1000) / AUDIO_SAMPLE_RATE as u64;
                    info!("VAD: speech ended ({} ms captured)", duration_ms);
                    let _ = event_tx.send(VadEvent::SpeechEnd(captured)).await;
                    last_voiced = None;
                }
            }
        }
    }

    debug!("VAD processor stopped (audio channel closed)");
}

/// Returns true if the frame contains voiced audio.
fn score_frame(vad: &mut VoiceActivityDetector, frame: &[f32]) -> bool {
    // Process in 30ms chunks (480 samples at 16kHz)
    let chunk_size = 480;
    let mut any_voiced = false;

    for chunk in frame.chunks(chunk_size) {
        if chunk.len() < chunk_size {
            break; // skip incomplete last chunk
        }
        // Convert f32 [-1, 1] to i16 for VAD
        let i16_samples: Vec<i16> = chunk
            .iter()
            .map(|&s| (s * i16::MAX as f32) as i16)
            .collect();

        if let Ok(probability) = vad.predict(i16_samples) {
            if probability > 0.5 {
                any_voiced = true;
            }
        }
    }

    any_voiced
}
