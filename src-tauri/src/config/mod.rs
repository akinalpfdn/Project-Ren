pub mod defaults;

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::info;

use defaults::*;

/// Runtime application configuration.
/// Persisted to `%APPDATA%\Ren\config.json`.
/// All fields have sensible defaults — missing keys are filled on load.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    // Audio
    pub sample_rate: u32,
    pub channels: u16,

    // Wake word
    pub wake_sensitivity: f32,

    // Conversation
    pub conversation_timeout_secs: u64,

    // TTS
    pub tts_voice: String,

    // Ollama
    pub ollama_port: Option<u16>,
    pub ollama_model: String,

    // Web tools
    pub brave_api_key: Option<String>,
    pub location: Option<String>,

    // System
    pub autostart: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            sample_rate: AUDIO_SAMPLE_RATE,
            channels: AUDIO_CHANNELS,
            wake_sensitivity: WAKE_WORD_SENSITIVITY,
            conversation_timeout_secs: CONVERSATION_IDLE_TIMEOUT_SECS,
            tts_voice: TTS_DEFAULT_VOICE.to_string(),
            ollama_port: None,
            ollama_model: OLLAMA_MODEL.to_string(),
            brave_api_key: None,
            location: None,
            autostart: false,
        }
    }
}

impl AppConfig {
    /// Load config from disk, or create default if missing.
    pub fn load() -> Result<Self> {
        let path = config_path()?;

        if !path.exists() {
            let config = AppConfig::default();
            config.save()?;
            info!("Created default config at {}", path.display());
            return Ok(config);
        }

        let bytes = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config at {}", path.display()))?;

        let config: AppConfig = serde_json::from_str(&bytes)
            .with_context(|| "Failed to parse config.json — file may be corrupt")?;

        info!("Loaded config from {}", path.display());
        Ok(config)
    }

    /// Persist current config to disk.
    pub fn save(&self) -> Result<()> {
        let path = config_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory at {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;

        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;

        Ok(())
    }
}

/// Returns the path to `%APPDATA%\Ren\config.json`.
pub fn config_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("config.json"))
}

/// Returns the `%APPDATA%\Ren\` base directory, creating it if missing.
pub fn app_data_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "ren", "Ren")
        .context("Failed to resolve application data directory")?;
    let path = dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(&path)
        .with_context(|| format!("Failed to create app data directory at {}", path.display()))?;
    Ok(path)
}

/// Returns `%APPDATA%\Ren\models\`.
pub fn models_dir() -> Result<PathBuf> {
    let path = app_data_dir()?.join("models");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Returns `%APPDATA%\Ren\logs\`.
pub fn logs_dir() -> Result<PathBuf> {
    let path = app_data_dir()?.join("logs");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Returns `%APPDATA%\Ren\bin\`.
pub fn bin_dir() -> Result<PathBuf> {
    let path = app_data_dir()?.join("bin");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Returns `%APPDATA%\Ren\cache\`.
pub fn cache_dir() -> Result<PathBuf> {
    let path = app_data_dir()?.join("cache");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}
