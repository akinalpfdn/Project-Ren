pub mod whisper;

use anyhow::Result;
use async_trait::async_trait;

/// Common interface for speech-to-text engines.
/// Abstracted so the engine can be swapped (e.g. faster-whisper, cloud fallback).
#[async_trait]
pub trait SttEngine: Send + Sync {
    /// Transcribe raw f32 PCM audio (16kHz mono) and return the text.
    async fn transcribe(&self, audio: &[f32]) -> Result<String>;

    /// Load the model into memory. Called lazily on first use.
    async fn load(&mut self) -> Result<()>;

    /// Unload the model to free memory/VRAM. Called when returning to Sleeping.
    fn unload(&mut self);

    /// Whether the model is currently loaded and ready.
    fn is_loaded(&self) -> bool;
}
