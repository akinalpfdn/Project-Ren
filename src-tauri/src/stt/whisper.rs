use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::{info, warn};

use crate::config::{models_dir, defaults::WHISPER_MODEL_FILENAME};
use super::SttEngine;

/// Whisper-based STT engine wrapping whisper.cpp via whisper-rs.
///
/// Guarded by the `stt` feature flag — on machines without whisper.cpp
/// built (e.g. low-spec work computers), the type still compiles but
/// `load()` returns an error with a clear message.
pub struct WhisperEngine {
    /// Underlying context — None until `load()` is called.
    #[cfg(feature = "stt")]
    ctx: Option<whisper_rs::WhisperContext>,

    /// Whether the model is currently loaded.
    loaded: bool,
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "stt")]
            ctx: None,
            loaded: false,
        }
    }

    /// Returns the path to the Whisper model file.
    fn model_path() -> Result<std::path::PathBuf> {
        Ok(models_dir()?.join("whisper").join(WHISPER_MODEL_FILENAME))
    }
}

#[async_trait]
impl SttEngine for WhisperEngine {
    async fn load(&mut self) -> Result<()> {
        if self.loaded {
            return Ok(());
        }

        #[cfg(feature = "stt")]
        {
            let model_path = Self::model_path()?;

            if !model_path.exists() {
                anyhow::bail!(
                    "Whisper model not found at {}. Run first-time setup to download it.",
                    model_path.display()
                );
            }

            let path_str = model_path
                .to_str()
                .context("Model path contains invalid UTF-8")?
                .to_string();

            // Load runs synchronously inside spawn_blocking to avoid blocking the async runtime
            let ctx = tokio::task::spawn_blocking(move || {
                whisper_rs::WhisperContext::new_with_params(
                    &path_str,
                    whisper_rs::WhisperContextParameters::default(),
                )
            })
            .await
            .context("Whisper model load task panicked")?
            .context("Failed to load Whisper model")?;

            self.ctx = Some(ctx);
            self.loaded = true;
            info!("Whisper model loaded (large-v3)");
            return Ok(());
        }

        #[cfg(not(feature = "stt"))]
        {
            anyhow::bail!(
                "STT feature not enabled. \
                 Recompile with --features stt on a machine with whisper.cpp built."
            );
        }
    }

    async fn transcribe(&self, audio: &[f32]) -> Result<String> {
        if !self.loaded {
            anyhow::bail!("Whisper model is not loaded — call load() first");
        }

        #[cfg(feature = "stt")]
        {
            let ctx = self
                .ctx
                .as_ref()
                .context("WhisperContext unexpectedly None")?;

            let mut state = ctx
                .create_state()
                .context("Failed to create Whisper state")?;

            let params = {
                let mut p = whisper_rs::FullParams::new(
                    whisper_rs::SamplingStrategy::Greedy { best_of: 1 },
                );
                p.set_language(Some("tr"));
                p.set_translate(false);
                p.set_print_progress(false);
                p.set_print_realtime(false);
                p.set_print_special(false);
                // Suppress Whisper's Turkish-subtitle hallucination ("Altyazı: M.K.",
                // "Altyazı çevirmeni: …") which surfaces on pauses and silent tail-ends.
                // Higher no_speech threshold skips low-confidence silent windows;
                // suppress_blank kills empty-token predictions; the initial prompt
                // anchors the model to a command domain instead of movie credits.
                p.set_no_speech_thold(0.6);
                p.set_suppress_blank(true);
                p.set_initial_prompt(
                    "Kısa sesli komutlar. Asistan konuşması. Film altyazısı değildir.",
                );
                p
            };

            let audio_owned = audio.to_vec();
            let text = tokio::task::spawn_blocking(move || -> Result<String> {
                state
                    .full(params, &audio_owned)
                    .context("Whisper transcription failed")?;

                let num_segments = state.full_n_segments();
                let mut result = String::new();
                for i in 0..num_segments {
                    let segment = state
                        .get_segment(i)
                        .context("Whisper segment index out of range")?;
                    result.push_str(
                        segment
                            .to_str()
                            .context("Whisper segment text not valid UTF-8")?,
                    );
                }
                Ok(result.trim().to_string())
            })
            .await
            .context("Whisper transcription task panicked")??;

            info!("Transcript: {:?}", text);
            return Ok(text);
        }

        #[cfg(not(feature = "stt"))]
        {
            anyhow::bail!("STT feature not enabled");
        }
    }

    fn unload(&mut self) {
        #[cfg(feature = "stt")]
        {
            if self.loaded {
                self.ctx = None;
                self.loaded = false;
                info!("Whisper model unloaded");
            }
        }
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }
}
