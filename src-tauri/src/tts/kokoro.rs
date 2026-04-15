//! HTTP client for the `ren-tts` sidecar.
//!
//! ORT and whisper.cpp both bring their own CUDA backend and refuse to share
//! one process — initialising both crashes with `STATUS_STACK_BUFFER_OVERRUN`.
//! Kokoro therefore lives in a separate `ren-tts.exe` child managed by
//! `tts::process` (Job-Object-supervised, mirrors `llm::ollama_process`).
//!
//! `KokoroEngine` here is a thin client: it holds the sidecar URL + the
//! default voice and forwards `synthesize()` calls over HTTP. The model
//! never loads in this process; `load()` only health-checks the sidecar.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::info;

use super::{process, AudioBuffer, TtsEngine};
use crate::config::defaults::TTS_DEFAULT_VOICE;

const KOKORO_SAMPLE_RATE: u32 = 24_000;
const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
const SYNTHESIZE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct KokoroEngine {
    voice: String,
    client: reqwest::Client,
    loaded: AtomicBool,
}

impl KokoroEngine {
    /// The sidecar URL is resolved on every call via [`process::base_url`]
    /// so the engine works whether the sidecar comes up before or after
    /// the engine is constructed.
    pub fn new(voice: Option<&str>) -> Self {
        Self {
            voice: voice.unwrap_or(TTS_DEFAULT_VOICE).to_string(),
            client: reqwest::Client::builder()
                .timeout(SYNTHESIZE_TIMEOUT)
                .build()
                .expect("reqwest client builds with static config"),
            loaded: AtomicBool::new(false),
        }
    }

    fn current_base_url(&self) -> Result<String> {
        process::base_url().context("ren-tts sidecar has not reported a port yet")
    }
}

#[async_trait]
impl TtsEngine for KokoroEngine {
    async fn load(&mut self) -> Result<()> {
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        let base = self.current_base_url()?;
        let url = format!("{}/health", base);
        let resp = self
            .client
            .get(&url)
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await
            .with_context(|| format!("ren-tts health check failed at {}", url))?;
        if !resp.status().is_success() {
            anyhow::bail!("ren-tts /health returned {}", resp.status());
        }
        self.loaded.store(true, Ordering::Release);
        info!("ren-tts sidecar reachable at {}", base);
        Ok(())
    }

    async fn synthesize(&self, text: &str) -> Result<AudioBuffer> {
        if !self.loaded.load(Ordering::Acquire) {
            anyhow::bail!("ren-tts sidecar not loaded — call load() first");
        }

        let base = self.current_base_url()?;
        let url = format!("{}/synthesize", base);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "text": text,
                "voice": self.voice,
            }))
            .send()
            .await
            .with_context(|| format!("ren-tts synthesize request failed at {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("ren-tts /synthesize returned {}: {}", status, body);
        }

        let bytes = resp
            .bytes()
            .await
            .context("ren-tts /synthesize body read failed")?;

        if bytes.len() % 4 != 0 {
            anyhow::bail!(
                "ren-tts returned {} bytes, not a multiple of 4 (expected f32 PCM)",
                bytes.len()
            );
        }

        let mut samples = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let arr = [chunk[0], chunk[1], chunk[2], chunk[3]];
            samples.push(f32::from_le_bytes(arr));
        }

        // Debug dump — overwritten on every turn. Lets us play the raw
        // Kokoro output in an external player and confirm whether any
        // leading audio is actually missing from the buffer we hand to
        // playback, versus being clipped by the rodio/audio path.
        if let Ok(log_dir) = crate::config::app_data_dir() {
            let dump = log_dir.join("logs").join("last_tts.wav");
            if let Err(e) = dump_wav(&dump, &samples, KOKORO_SAMPLE_RATE) {
                tracing::warn!("TTS dump failed: {}", e);
            }
        }

        Ok(samples)
    }

    fn unload(&mut self) {
        // Sidecar owns the model; nothing to free here. Mark unloaded so a
        // future call re-checks /health (sidecar may have died/restarted).
        self.loaded.store(false, Ordering::Release);
    }

    fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }

    fn sample_rate(&self) -> u32 {
        KOKORO_SAMPLE_RATE
    }
}

/// Minimal 32-bit float PCM WAV writer for diagnostic dumps.
fn dump_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut f = std::fs::File::create(path).context("create wav dump")?;
    let data_size = (samples.len() * 4) as u32;
    let riff_size = 36 + data_size;
    f.write_all(b"RIFF")?;
    f.write_all(&riff_size.to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&3u16.to_le_bytes())?; // IEEE float
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&sample_rate.to_le_bytes())?;
    let byte_rate = sample_rate * 4;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?; // block align
    f.write_all(&32u16.to_le_bytes())?; // bits per sample
    f.write_all(b"data")?;
    f.write_all(&data_size.to_le_bytes())?;
    for s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}
