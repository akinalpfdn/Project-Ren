//! Porcupine-backed `WakeEngine` implementation.
//!
//! Compiled in two flavours via the `wake` feature flag:
//!
//! - With `wake`: real `pv_porcupine` integration. Requires the Picovoice
//!   native library at link time and a valid access key at runtime.
//! - Without `wake`: every method returns a clear "feature disabled" error
//!   so the rest of the binary still compiles and runs.
//!
//! The split mirrors how `stt::whisper` and `tts::kokoro` handle their own
//! optional native dependencies.

use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

use super::{WakeEngine, WakeEvent, WakeKeyword};

/// Wake engine driven by the Picovoice Porcupine SDK.
pub struct PorcupineWakeEngine {
    keywords: Vec<WakeKeyword>,
    access_key: String,
    #[cfg(feature = "wake")]
    porcupine: Option<pv_porcupine::Porcupine>,
    #[cfg(not(feature = "wake"))]
    loaded: bool,
}

impl PorcupineWakeEngine {
    pub fn new(access_key: impl Into<String>, keywords: Vec<WakeKeyword>) -> Self {
        Self {
            keywords,
            access_key: access_key.into(),
            #[cfg(feature = "wake")]
            porcupine: None,
            #[cfg(not(feature = "wake"))]
            loaded: false,
        }
    }
}

#[cfg(feature = "wake")]
#[async_trait]
impl WakeEngine for PorcupineWakeEngine {
    async fn load(&mut self) -> Result<()> {
        if self.porcupine.is_some() {
            return Ok(());
        }
        if self.access_key.is_empty() {
            anyhow::bail!(
                "Picovoice access key missing — set PICOVOICE_ACCESS_KEY at build time"
            );
        }
        if self.keywords.is_empty() {
            anyhow::bail!("PorcupineWakeEngine requires at least one keyword");
        }

        let access_key = self.access_key.clone();
        let keywords = self.keywords.clone();

        let porcupine = tokio::task::spawn_blocking(move || {
            let model_paths: Vec<String> =
                keywords.iter().map(|k| k.model_path.clone()).collect();
            let sensitivities: Vec<f32> =
                keywords.iter().map(|k| k.sensitivity).collect();

            pv_porcupine::PorcupineBuilder::new_with_keyword_paths(access_key, &model_paths)
                .sensitivities(&sensitivities)
                .init()
        })
        .await
        .map_err(|e| anyhow::anyhow!("Porcupine init join failed: {}", e))?
        .map_err(|e| anyhow::anyhow!("Porcupine init failed: {:?}", e))?;

        info!(
            "Porcupine loaded with {} keyword(s)",
            self.keywords.len()
        );
        self.porcupine = Some(porcupine);
        Ok(())
    }

    async fn unload(&mut self) -> Result<()> {
        self.porcupine = None;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.porcupine.is_some()
    }

    fn process(&mut self, frame: &[i16]) -> Result<Option<WakeEvent>> {
        let porcupine = self
            .porcupine
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Porcupine engine not loaded"))?;

        let expected = porcupine.frame_length() as usize;
        if frame.len() != expected {
            anyhow::bail!(
                "Wake frame length mismatch: got {}, expected {}",
                frame.len(),
                expected
            );
        }

        let index = porcupine
            .process(frame)
            .map_err(|e| anyhow::anyhow!("Porcupine.process failed: {:?}", e))?;

        if index < 0 {
            return Ok(None);
        }

        let keyword = self
            .keywords
            .get(index as usize)
            .ok_or_else(|| anyhow::anyhow!("Porcupine returned out-of-range keyword index"))?;

        Ok(Some(WakeEvent {
            keyword_id: keyword.id.clone(),
        }))
    }

    fn frame_length(&self) -> usize {
        self.porcupine
            .as_ref()
            .map(|p| p.frame_length() as usize)
            .unwrap_or(512)
    }

    fn sample_rate(&self) -> u32 {
        self.porcupine
            .as_ref()
            .map(|p| p.sample_rate() as u32)
            .unwrap_or(16_000)
    }
}

#[cfg(not(feature = "wake"))]
#[async_trait]
impl WakeEngine for PorcupineWakeEngine {
    async fn load(&mut self) -> Result<()> {
        anyhow::bail!(
            "Wake word detection requires the 'wake' feature — rebuild with --features wake"
        )
    }

    async fn unload(&mut self) -> Result<()> {
        self.loaded = false;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn process(&mut self, _frame: &[i16]) -> Result<Option<WakeEvent>> {
        anyhow::bail!("Wake word detection disabled (build without 'wake' feature)")
    }

    fn frame_length(&self) -> usize {
        512
    }

    fn sample_rate(&self) -> u32 {
        16_000
    }
}
