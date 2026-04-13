/// All default configuration values in one place.
/// Change a default here — it applies everywhere.

// Application
/// Folder name under `%APPDATA%` (Windows) / `$XDG_DATA_HOME` (Linux).
/// Produces `%APPDATA%\Ren\` — the documented, single-segment layout.
pub const APP_DIR_NAME: &str = "Ren";

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

// Wake word resource filenames (bundled as Tauri resources at build time).
pub const WAKE_KEYWORD_HEY_REN: &str = "hey_ren_en_windows.ppn";
pub const WAKE_KEYWORD_REN_UYAN: &str = "ren_uyan_en_windows.ppn";

// Picovoice access key. Resolved at compile time from `PICOVOICE_ACCESS_KEY`
// (set by CI / build script). Empty in the source tree. Standard practice for
// Porcupine personal use; not treated as a secret by Picovoice.
pub const PICOVOICE_ACCESS_KEY: &str = match option_env!("PICOVOICE_ACCESS_KEY") {
    Some(v) => v,
    None => "",
};

// Acknowledgement chime played on wake (bundled resource).
pub const WAKE_ACK_SOUND: &str = "wake_ack.wav";

// Dismissal phrases — case-insensitive substring match against the user transcript.
// Any match while Ren is awake forces transition to Sleeping immediately.
pub const DISMISSAL_PHRASES: &[&str] = &[
    // English
    "go to sleep",
    "goodbye ren",
    "bye ren",
    "thats all",
    "that's all",
    "that is all",
    "thank you ren",
    "sleep now",
    // Turkish
    "görüşürüz",
    "tamam yeter",
    "uyu artık",
    "iyi geceler",
    "kapan",
];

// Download
pub const DOWNLOAD_CHUNK_SIZE: usize = 8 * 1024 * 1024; // 8 MB

// SHA256 hashes for model integrity verification.
// Update these whenever model sources publish a new file.
pub const WHISPER_LARGE_V3_SHA256: &str =
    "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2";
