pub mod capture;
pub mod vad;
pub mod wake_consumer;

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};
use tracing::info;

use capture::AudioSamples;
use vad::VadEvent;

use crate::state::SharedStateMachine;
use crate::wake::{WakeEngine, WakeEvent};

/// Optional wake-detection plumbing passed into the pipeline. Set to `None`
/// when the `wake` feature is disabled or Picovoice resources are missing at
/// startup — the pipeline then behaves exactly like the Phase 2 build.
pub struct WakeHookup {
    pub engine: Arc<Mutex<dyn WakeEngine>>,
    pub event_tx: mpsc::Sender<WakeEvent>,
    pub state_machine: SharedStateMachine,
}

/// Starts the audio pipeline: capture → fan-out → VAD (+ optional wake).
/// Returns the cpal Stream handle, which MUST be kept alive for audio to flow.
pub fn start_pipeline(
    vad_event_tx: mpsc::Sender<VadEvent>,
    wake: Option<WakeHookup>,
) -> Result<cpal::Stream> {
    let frame_duration_ms = crate::config::defaults::VAD_FRAME_SIZE_MS;

    let (stream, mut audio_rx) = capture::start_capture(frame_duration_ms)?;

    // Internal fan-out: one upstream frame is cloned to each downstream
    // consumer. Bounded channels apply back-pressure to the fan-out task
    // rather than to the cpal callback, which must never block.
    let (vad_audio_tx, vad_audio_rx) = mpsc::channel::<AudioSamples>(64);
    let (wake_audio_tx, wake_audio_rx) = if wake.is_some() {
        let (tx, rx) = mpsc::channel::<AudioSamples>(64);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    tauri::async_runtime::spawn(async move {
        while let Some(frame) = audio_rx.recv().await {
            if let Some(tx) = &wake_audio_tx {
                // Cloning the Vec is O(N) but at 16 kHz mono the per-frame
                // cost (~1.9 kB / 30 ms) is negligible versus inference.
                let _ = tx.try_send(frame.clone());
            }
            if vad_audio_tx.send(frame).await.is_err() {
                break;
            }
        }
    });

    tauri::async_runtime::spawn(async move {
        vad::run(vad_audio_rx, vad_event_tx).await;
    });

    if let (Some(hookup), Some(wake_rx)) = (wake, wake_audio_rx) {
        tauri::async_runtime::spawn(async move {
            wake_consumer::run(
                wake_rx,
                hookup.engine,
                hookup.event_tx,
                hookup.state_machine,
            )
            .await;
        });
    }

    info!("Audio pipeline started");
    Ok(stream)
}
