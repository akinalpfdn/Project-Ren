use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

use crate::config::{models_dir, defaults::{KOKORO_MODEL_FILENAME, TTS_DEFAULT_VOICE}};
use super::{AudioBuffer, TtsEngine};

const KOKORO_SAMPLE_RATE: u32 = 24_000;

/// Kokoro TTS engine backed by ONNX Runtime.
///
/// Feature-gated on `tts` — compiles as a stub on machines without ORT installed.
/// On a capable machine with `--features tts`, loads `kokoro.onnx` and synthesizes.
pub struct KokoroEngine {
    voice: String,

    #[cfg(feature = "tts")]
    session: Option<ort::Session>,

    loaded: bool,
}

impl KokoroEngine {
    pub fn new(voice: Option<&str>) -> Self {
        Self {
            voice: voice.unwrap_or(TTS_DEFAULT_VOICE).to_string(),
            #[cfg(feature = "tts")]
            session: None,
            loaded: false,
        }
    }

    fn model_path() -> Result<std::path::PathBuf> {
        Ok(models_dir()?.join("kokoro").join(KOKORO_MODEL_FILENAME))
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

            if !model_path.exists() {
                anyhow::bail!(
                    "Kokoro model not found at {}. Run first-time setup.",
                    model_path.display()
                );
            }

            let path = model_path
                .to_str()
                .context("Model path contains invalid UTF-8")?
                .to_string();

            let session = tokio::task::spawn_blocking(move || {
                ort::Session::builder()
                    .context("Failed to create ORT session builder")?
                    .commit_from_file(path)
                    .context("Failed to load Kokoro ONNX model")
            })
            .await
            .context("Kokoro load task panicked")??;

            self.session = Some(session);
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
            let _session = self
                .session
                .as_ref()
                .context("Kokoro session unexpectedly None")?;

            // TODO: implement tokenization + ORT inference when testing at home.
            // Kokoro expects:
            //   input_ids: [1, seq_len] int64 — phoneme token IDs
            //   style: [1, 256] float32 — voice style embedding for self.voice
            //   speed: [1] float32 — playback speed (1.0 = normal)
            // Output:
            //   audio: [1, samples] float32 — raw PCM at 24kHz
            //
            // Reference tokenizer: https://github.com/thewh1teagle/kokoro-onnx
            anyhow::bail!("Kokoro inference not yet implemented — complete at home");
        }

        #[cfg(not(feature = "tts"))]
        {
            anyhow::bail!("TTS feature not enabled");
        }
    }

    fn unload(&mut self) {
        #[cfg(feature = "tts")]
        {
            self.session = None;
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
