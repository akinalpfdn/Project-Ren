//! Wake word detection abstraction.
//!
//! Trait-based design lets the rest of the app stay engine-agnostic. The default
//! implementation is Porcupine (`porcupine` module), gated behind the `wake`
//! feature flag so the project compiles without the native Picovoice library.

pub mod porcupine;

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;

/// A keyword the engine listens for. Bundled as a Tauri resource (.ppn file)
/// produced by the Picovoice Console.
#[derive(Debug, Clone)]
pub struct WakeKeyword {
    /// Stable identifier used in logs and events ("hey_ren", "ren_uyan", ...).
    pub id: String,
    /// Path to the .ppn model file (resolved against the Tauri resource dir).
    pub model_path: String,
    /// Per-keyword sensitivity in the closed range [0.0, 1.0].
    /// Higher = more sensitive = more false positives.
    pub sensitivity: f32,
}

/// Event emitted when a wake keyword fires.
#[derive(Debug, Clone, Serialize)]
pub struct WakeEvent {
    /// Identifier of the keyword that triggered. Matches `WakeKeyword::id`.
    pub keyword_id: String,
}

/// Wake word engine contract. Implementations are responsible for their own
/// audio consumption — most plug into the same cpal stream the rest of the
/// pipeline uses.
#[async_trait]
pub trait WakeEngine: Send + Sync {
    /// Load model files and acquire any native resources. Idempotent.
    async fn load(&mut self) -> Result<()>;

    /// Drop loaded models and release native resources.
    async fn unload(&mut self) -> Result<()>;

    /// Returns `true` when `load()` has succeeded and the engine is ready to
    /// process audio frames.
    fn is_loaded(&self) -> bool;

    /// Feed one frame of 16 kHz mono i16 PCM audio. The frame size is engine
    /// specific (Porcupine uses 512 samples). Returns the keyword that
    /// triggered, if any.
    fn process(&mut self, frame: &[i16]) -> Result<Option<WakeEvent>>;

    /// Engine-required input frame length in samples. Callers must buffer audio
    /// to exactly this size before calling `process()`.
    fn frame_length(&self) -> usize;

    /// Engine-required sample rate. Must match the upstream capture rate.
    fn sample_rate(&self) -> u32;
}
