//! Ren TTS sidecar.
//!
//! A minimal HTTP server that owns a Kokoro TTS engine. The main `ren.exe`
//! spawns this binary and talks to it over localhost so ORT's CUDA backend
//! stays in its own process and never collides with whisper.cpp's CUDA
//! backend living inside the main app.
//!
//! Protocol:
//!   GET  /health       -> 200 once the model is loaded
//!   POST /synthesize   { "text": "...", "voice": "af_heart" }
//!     -> 200, body = raw f32 little-endian PCM samples
//!     -> headers: X-Sample-Rate, Content-Type: application/octet-stream
//!
//! Lifecycle:
//!   - Picks a port (CLI `--port` or first free in 11530..11550).
//!   - Prints the bound port to stdout as `ren-tts ready on port <N>`
//!     so the parent can parse it without scraping logs.
//!   - Loads the Kokoro model lazily on the first /synthesize call so
//!     /health returns fast and the parent can mark the process alive.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{info, warn};

const KOKORO_SAMPLE_RATE: u32 = 24_000;
const PORT_RANGE: std::ops::Range<u16> = 11530..11551;

#[derive(Parser, Debug)]
#[command(name = "ren-tts", about = "Kokoro TTS sidecar for Project Ren")]
struct Args {
    /// Bind to this localhost port; if omitted, scans 11530..11550.
    #[arg(long)]
    port: Option<u16>,

    /// Path to `kokoro.onnx`. Defaults to `%APPDATA%\Ren\models\kokoro\kokoro.onnx`.
    #[arg(long)]
    model: Option<PathBuf>,

    /// Path to `voices.bin`. Defaults to `%APPDATA%\Ren\models\kokoro\voices.bin`.
    #[arg(long)]
    voices: Option<PathBuf>,

    /// Default voice when a request omits it.
    #[arg(long, default_value = "af_heart")]
    default_voice: String,
}

/// Held behind an async mutex because Kokoro inference is single-threaded
/// and we don't want overlapping calls fighting over the model.
struct AppState {
    model_path: PathBuf,
    voices_path: PathBuf,
    default_voice: String,
    engine: Mutex<Option<kokoro_tiny::TtsEngine>>,
}

#[derive(Debug, Deserialize)]
struct SynthesizeRequest {
    text: String,
    #[serde(default)]
    voice: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let args = Args::parse();

    let model_path = args.model.unwrap_or_else(default_model_path);
    let voices_path = args.voices.unwrap_or_else(default_voices_path);

    if !model_path.exists() {
        anyhow::bail!(
            "Kokoro model missing: {}. Run first-time setup before launching the sidecar.",
            model_path.display()
        );
    }
    if !voices_path.exists() {
        anyhow::bail!(
            "Kokoro voices missing: {}. Run first-time setup before launching the sidecar.",
            voices_path.display()
        );
    }

    let state = Arc::new(AppState {
        model_path,
        voices_path,
        default_voice: args.default_voice,
        engine: Mutex::new(None),
    });

    let listener = bind_listener(args.port).await?;
    let bound = listener.local_addr().context("listener has no local addr")?;

    // The parent reads this single line to learn the port.
    println!("ren-tts ready on port {}", bound.port());
    info!("ren-tts listening on {}", bound);

    let app = Router::new()
        .route("/health", get(health))
        .route("/synthesize", post(synthesize))
        .with_state(state);

    axum::serve(listener, app)
        .await
        .context("axum serve crashed")?;

    Ok(())
}

async fn bind_listener(preferred: Option<u16>) -> Result<TcpListener> {
    if let Some(port) = preferred {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        return TcpListener::bind(addr)
            .await
            .with_context(|| format!("failed to bind requested port {}", port));
    }

    for port in PORT_RANGE {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        if let Ok(listener) = TcpListener::bind(addr).await {
            return Ok(listener);
        }
    }

    anyhow::bail!(
        "no free port in {}..{} for ren-tts",
        PORT_RANGE.start,
        PORT_RANGE.end
    )
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn synthesize(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SynthesizeRequest>,
) -> Result<(StatusCode, HeaderMap, Vec<u8>), (StatusCode, String)> {
    let text = req.text.trim();
    if text.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty text".into()));
    }

    let voice = req
        .voice
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| state.default_voice.clone());

    let mut guard = state.engine.lock().await;
    if guard.is_none() {
        info!("loading Kokoro model on first synthesize request");
        let model = state
            .model_path
            .to_str()
            .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "model path not UTF-8".into()))?
            .to_string();
        let voices = state
            .voices_path
            .to_str()
            .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "voices path not UTF-8".into()))?
            .to_string();
        let engine = kokoro_tiny::TtsEngine::with_paths(&model, &voices)
            .await
            .map_err(|e| {
                warn!("Kokoro load failed: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Kokoro load failed: {}", e),
                )
            })?;
        *guard = Some(engine);
    }

    // Synthesis is CPU-heavy and synchronous; drop the lock onto a
    // blocking task so the async runtime can keep serving /health while
    // it runs. The vendored kokoro-tiny fork adds the boundary padding
    // tokens Kokoro expects, so no filler prefix or trim is needed here.
    let engine = guard.take().expect("engine just loaded");
    let text_owned = text.to_string();
    let (engine, samples) = tokio::task::spawn_blocking(move || {
        let mut engine = engine;
        let result = engine.synthesize(&text_owned, Some(&voice));
        (engine, result)
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("synthesize task panicked: {}", e),
        )
    })?;
    *guard = Some(engine);

    let samples = samples.map_err(|e| {
        warn!("Kokoro synthesize failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Kokoro synthesize failed: {}", e),
        )
    })?;

    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        "X-Sample-Rate",
        HeaderValue::from_str(&KOKORO_SAMPLE_RATE.to_string()).unwrap(),
    );

    Ok((StatusCode::OK, headers, bytes))
}

fn default_model_path() -> PathBuf {
    models_dir().join("kokoro").join("kokoro.onnx")
}

fn default_voices_path() -> PathBuf {
    // Filename matches `KOKORO_VOICES_FILENAME` in the main app's config.
    models_dir().join("kokoro").join("voices-v1.0.bin")
}

fn models_dir() -> PathBuf {
    if let Some(base) = directories::BaseDirs::new() {
        // %APPDATA%\Ren\models on Windows.
        base.data_dir().join("Ren").join("models")
    } else {
        PathBuf::from(".")
    }
}
