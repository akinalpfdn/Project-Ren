pub mod kokoro;
pub mod process;

use anyhow::Result;
use async_trait::async_trait;

/// Raw f32 PCM audio (44100 Hz stereo or 24000 Hz mono depending on engine).
pub type AudioBuffer = Vec<f32>;

/// Common interface for text-to-speech engines.
/// Abstracted so the engine can be swapped (e.g. Piper for Turkish, XTTS-v2).
#[async_trait]
pub trait TtsEngine: Send + Sync {
    /// Synthesize text and return raw PCM audio.
    async fn synthesize(&self, text: &str) -> Result<AudioBuffer>;

    /// Load the model into memory. Called lazily on first use.
    async fn load(&mut self) -> Result<()>;

    /// Unload the model to free memory.
    fn unload(&mut self);

    /// Whether the model is currently loaded.
    fn is_loaded(&self) -> bool;

    /// Sample rate of the returned audio buffer.
    fn sample_rate(&self) -> u32;
}
