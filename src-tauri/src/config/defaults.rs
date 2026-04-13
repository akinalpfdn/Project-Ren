/// All default configuration values in one place.
/// Change a default here — it applies everywhere.

// Audio
pub const AUDIO_SAMPLE_RATE: u32 = 16_000;
pub const AUDIO_CHANNELS: u16 = 1;

// VAD
pub const VAD_SILENCE_THRESHOLD_MS: u64 = 500;
pub const VAD_FRAME_SIZE_MS: u64 = 30;

// Ollama
pub const OLLAMA_PREFERRED_PORT: u16 = 11_500;
pub const OLLAMA_PORT_PROBE_MAX: u16 = 11_520;
pub const OLLAMA_MODEL: &str = "qwen2.5:14b";
pub const OLLAMA_KEEP_ALIVE: &str = "30m";
pub const OLLAMA_HEALTH_CHECK_INTERVAL_MS: u64 = 500;
pub const OLLAMA_HEALTH_CHECK_TIMEOUT_SECS: u64 = 30;

// Pinned Ollama binary version (update when bumping)
pub const OLLAMA_PINNED_VERSION: &str = "0.9.0";

// Models
pub const WHISPER_MODEL_FILENAME: &str = "ggml-large-v3.bin";
pub const KOKORO_MODEL_FILENAME: &str = "kokoro.onnx";

// TTS
pub const TTS_DEFAULT_VOICE: &str = "bf_emma";

// Conversation
pub const CONVERSATION_IDLE_TIMEOUT_SECS: u64 = 30;

// Wake word sensitivity (0.0–1.0, higher = more sensitive = more false positives)
pub const WAKE_WORD_SENSITIVITY: f32 = 0.5;

// Download
pub const DOWNLOAD_CHUNK_SIZE: usize = 8 * 1024 * 1024; // 8 MB

// SHA256 hashes for model integrity verification.
// Update these whenever model sources publish a new file.
pub const WHISPER_LARGE_V3_SHA256: &str =
    "964ef9a7b601b6847c71ba5d2d0f7e4f41cd5eed99b86e73c9b0bd0e9f69c8ec";
