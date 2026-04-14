use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

#[cfg(feature = "tts")]
use std::sync::{Arc, Mutex};

use super::{AudioBuffer, TtsEngine};
use crate::config::{
    defaults::{KOKORO_MODEL_FILENAME, KOKORO_VOICES_FILENAME, TTS_DEFAULT_VOICE},
    models_dir,
};

const KOKORO_SAMPLE_RATE: u32 = 24_000;

/// Kokoro TTS engine backed by the `kokoro-tiny` crate.
///
/// Feature-gated on `tts` — compiles as a stub on machines without ORT installed.
/// On a capable machine with `--features tts`, wraps `kokoro_tiny::TtsEngine`
/// which bundles espeak-rs for phonemization, ORT-based inference, and
/// precomputed voice style embeddings.
pub struct KokoroEngine {
    voice: String,

    #[cfg(feature = "tts")]
    engine: Option<Arc<Mutex<kokoro_tiny::TtsEngine>>>,

    loaded: bool,
}

impl KokoroEngine {
    pub fn new(voice: Option<&str>) -> Self {
        Self {
            voice: voice.unwrap_or(TTS_DEFAULT_VOICE).to_string(),
            #[cfg(feature = "tts")]
            engine: None,
            loaded: false,
        }
    }

    fn model_path() -> Result<std::path::PathBuf> {
        Ok(models_dir()?.join("kokoro").join(KOKORO_MODEL_FILENAME))
    }

    fn voices_path() -> Result<std::path::PathBuf> {
        Ok(models_dir()?.join("kokoro").join(KOKORO_VOICES_FILENAME))
    }
}

#[async_trait]
impl TtsEngine for KokoroEngine {
    async fn load(&mut self) -> Result<()> {
        if self.loaded {
            return Ok(());
        }

        #[cfg(feature = "tts")]
        {
            let model_path = Self::model_path()?;
            let voices_path = Self::voices_path()?;

            if !model_path.exists() {
                anyhow::bail!(
                    "Kokoro model not found at {}. Run first-time setup.",
                    model_path.display()
                );
            }
            if !voices_path.exists() {
                anyhow::bail!(
                    "Kokoro voices file not found at {}. Run first-time setup.",
                    voices_path.display()
                );
            }

            let model_str = model_path
                .to_str()
                .context("Kokoro model path contains invalid UTF-8")?
                .to_string();
            let voices_str = voices_path
                .to_str()
                .context("Kokoro voices path contains invalid UTF-8")?
                .to_string();

            let engine = kokoro_tiny::TtsEngine::with_paths(&model_str, &voices_str)
                .await
                .map_err(|e| anyhow::anyhow!("Kokoro load failed: {}", e))?;

            self.engine = Some(Arc::new(Mutex::new(engine)));
            self.loaded = true;
            info!("Kokoro model loaded (voice: {})", self.voice);
            return Ok(());
        }

        #[cfg(not(feature = "tts"))]
        {
            anyhow::bail!(
                "TTS feature not enabled. Recompile with --features tts on a machine with ORT installed."
            );
        }
    }

    async fn synthesize(&self, text: &str) -> Result<AudioBuffer> {
        if !self.loaded {
            anyhow::bail!("Kokoro model not loaded — call load() first");
        }

        #[cfg(feature = "tts")]
        {
            let engine = self
                .engine
                .as_ref()
                .context("Kokoro engine unexpectedly None")?
                .clone();
            let text = text.to_string();
            let voice = self.voice.clone();

            let audio = tokio::task::spawn_blocking(move || -> Result<Vec<f32>> {
                let mut guard = engine
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Kokoro engine mutex poisoned"))?;
                guard
                    .synthesize(&text, Some(&voice))
                    .map_err(|e| anyhow::anyhow!("Kokoro synthesize failed: {}", e))
            })
            .await
            .context("Kokoro synthesize task panicked")??;

            return Ok(audio);
        }

        #[cfg(not(feature = "tts"))]
        {
            anyhow::bail!("TTS feature not enabled");
        }
    }

    fn unload(&mut self) {
        #[cfg(feature = "tts")]
        {
            self.engine = None;
        }
        if self.loaded {
            self.loaded = false;
            info!("Kokoro model unloaded");
        }
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn sample_rate(&self) -> u32 {
        KOKORO_SAMPLE_RATE
    }
}
