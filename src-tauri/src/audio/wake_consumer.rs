//! Wake-word consumer task.
//!
//! Sits downstream of the capture stage and alongside VAD:
//!
//! ```text
//! cpal callback → 16 kHz mono f32 frames → audio fan-out
//!                                           ├→ VAD (speech boundaries)
//!                                           └→ wake consumer (this module)
//! ```
//!
//! Buffers incoming f32 frames, requantises to i16, and hands exact
//! `WakeEngine::frame_length()`-sized chunks to the engine. When the engine
//! fires, the event is forwarded onto `event_tx`.
//!
//! Processing is gated on `RenState::Sleeping` — the engine sits idle in every
//! other state so we do not waste CPU detecting the wake word while Ren is
//! already listening or speaking.
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use crate::audio::capture::AudioSamples;
use crate::state::{RenState, SharedStateMachine};
use crate::wake::{WakeEngine, WakeEvent};

/// Drives wake detection from the capture fan-out.
///
/// Exits cleanly when the audio channel closes (app shutdown).
pub async fn run(
    mut audio_rx: mpsc::Receiver<AudioSamples>,
    engine: Arc<Mutex<dyn WakeEngine>>,
    event_tx: mpsc::Sender<WakeEvent>,
    state_machine: SharedStateMachine,
) {
    let frame_length = {
        let guard = engine.lock().await;
        guard.frame_length()
    };

    let mut pcm: Vec<i16> = Vec::with_capacity(frame_length * 4);
    info!(
        "Wake consumer started (engine frame length: {} samples)",
        frame_length
    );

    while let Some(frame) = audio_rx.recv().await {
        let sleeping = matches!(
            state_machine.lock().unwrap().current(),
            RenState::Sleeping
        );
        if !sleeping {
            // Keep the buffer small so we do not resume detection on a stale
            // recording once Ren goes back to Sleeping.
            pcm.clear();
            continue;
        }

        pcm.extend(frame.iter().map(float_to_pcm16));

        while pcm.len() >= frame_length {
            let chunk: Vec<i16> = pcm.drain(..frame_length).collect();
            let mut guard = engine.lock().await;
            match guard.process(&chunk) {
                Ok(Some(event)) => {
                    info!("Wake word fired: {}", event.keyword_id);
                    if event_tx.send(event).await.is_err() {
                        warn!("Wake event channel closed — stopping wake consumer");
                        return;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("Wake engine process error: {}", e);
                }
            }
        }
    }

    debug!("Wake consumer stopped (audio channel closed)");
}

/// Converts a normalized f32 sample in `[-1.0, 1.0]` to 16-bit PCM with
/// saturating arithmetic so clipping does not overflow.
#[inline]
fn float_to_pcm16(sample: &f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32) as i16
}
