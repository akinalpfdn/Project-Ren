pub mod capture;
pub mod vad;

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::info;

use capture::AudioSamples;
use vad::VadEvent;

/// Starts the audio pipeline: capture → VAD.
/// Returns the VAD event receiver and the cpal Stream handle.
/// The stream handle MUST be kept alive for audio to flow.
pub fn start_pipeline(
    vad_event_tx: mpsc::Sender<VadEvent>,
) -> Result<cpal::Stream> {
    let frame_duration_ms = crate::config::defaults::VAD_FRAME_SIZE_MS;

    let (stream, audio_rx) = capture::start_capture(frame_duration_ms)?;

    // Spawn VAD task on Tauri's async runtime (this runs in the sync setup closure).
    tauri::async_runtime::spawn(async move {
        vad::run(audio_rx, vad_event_tx).await;
    });

    info!("Audio pipeline started");
    Ok(stream)
}
