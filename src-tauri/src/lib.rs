// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
mod config;
mod download;
mod hotkey;
mod llm;
mod playback;
mod state;
mod stt;
mod tts;

use std::process::Child;
use std::sync::{Arc, Mutex};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    audio::vad::VadEvent,
    hotkey::HotkeyEvent,
    llm::{conversation::Conversation, default_client},
    playback::AudioPlayer,
    state::{RenState, SharedStateMachine},
    stt::whisper::WhisperEngine,
    tts::kokoro::KokoroEngine,
};

#[derive(Clone, serde::Serialize)]
struct TranscriptPayload {
    text: String,
    is_final: bool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Ren starting up");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            let config = config::AppConfig::load().unwrap_or_else(|e| {
                warn!("Config load failed ({}), using defaults", e);
                config::AppConfig::default()
            });

            let state_machine = state::new_shared(app_handle.clone());
            app.manage(state_machine.clone());

            setup_tray(app)?;
            position_window_bottom_right(&app_handle)?;

            let (vad_event_tx, vad_event_rx) = mpsc::channel::<VadEvent>(32);
            let (hotkey_event_tx, hotkey_event_rx) = mpsc::channel::<HotkeyEvent>(16);
            let (sentence_tx, sentence_rx) = mpsc::channel::<String>(32);
            let (llm_token_tx, _llm_token_rx) = mpsc::channel::<String>(256);

            // Audio pipeline
            let _audio_stream = audio::start_pipeline(vad_event_tx)
                .expect("Failed to start audio pipeline");

            // Hotkeys
            let _hotkey_manager = hotkey::start(hotkey_event_tx)
                .expect("Failed to register hotkeys");

            // Engines (lazily loaded)
            let whisper = Arc::new(Mutex::new(WhisperEngine::new()));
            let kokoro = Arc::new(Mutex::new(KokoroEngine::new(
                Some(config.tts_voice.as_str()),
            )));

            // Audio player for TTS output
            let player = Arc::new(
                AudioPlayer::new().expect("Failed to initialize audio output"),
            );

            // Ollama child process (started async after setup)
            let ollama_child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
            let ollama_child_clone = ollama_child.clone();
            let sm_for_ollama = state_machine.clone();
            tokio::spawn(async move {
                match llm::ollama_process::start().await {
                    Ok(child) => {
                        *ollama_child_clone.lock().unwrap() = Some(child);
                        info!("Ollama child process ready");
                    }
                    Err(e) => {
                        warn!("Ollama not started: {} — LLM responses disabled", e);
                        // Not a fatal error during dev — STT still works without Ollama
                    }
                }
            });

            // TTS sentence consumer
            let kokoro_clone = kokoro.clone();
            let player_clone = player.clone();
            let sm_for_tts = state_machine.clone();
            let app_for_tts = app_handle.clone();
            tokio::spawn(async move {
                tts_sentence_loop(
                    app_for_tts,
                    sm_for_tts,
                    sentence_rx,
                    kokoro_clone,
                    player_clone,
                )
                .await;
            });

            // Main event loop
            let sm = state_machine.clone();
            let handle = app_handle.clone();
            let conversation = Arc::new(Mutex::new(Conversation::new()));
            tokio::spawn(async move {
                event_loop(
                    handle,
                    sm,
                    vad_event_rx,
                    hotkey_event_rx,
                    whisper,
                    sentence_tx,
                    llm_token_tx,
                    conversation,
                )
                .await;
            });

            // Transition to Sleeping once setup is complete
            state_machine
                .lock()
                .unwrap()
                .transition(RenState::Sleeping)
                .unwrap_or_else(|e| warn!("Init transition failed: {}", e));

            // Clean up Ollama on app exit
            let ollama_on_exit = ollama_child.clone();
            app.on_window_event(move |_, event| {
                if matches!(event, tauri::WindowEvent::Destroyed) {
                    if let Some(child) = ollama_on_exit.lock().unwrap().as_mut() {
                        llm::ollama_process::terminate(child);
                    }
                }
            });

            info!("Ren initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::toggle_window,
            commands::show_window,
            commands::hide_window,
            commands::get_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ─── Event loop ──────────────────────────────────────────────────────────────

async fn event_loop(
    app: AppHandle,
    sm: SharedStateMachine,
    mut vad_rx: mpsc::Receiver<VadEvent>,
    mut hotkey_rx: mpsc::Receiver<HotkeyEvent>,
    whisper: Arc<Mutex<WhisperEngine>>,
    sentence_tx: mpsc::Sender<String>,
    llm_token_tx: mpsc::Sender<String>,
    conversation: Arc<Mutex<Conversation>>,
) {
    let mut push_to_talk_active = false;

    loop {
        tokio::select! {
            Some(vad_event) = vad_rx.recv() => {
                handle_vad_event(
                    &app, &sm, vad_event, &whisper,
                    &sentence_tx, &llm_token_tx, &conversation,
                )
                .await;
            }
            Some(hotkey_event) = hotkey_rx.recv() => {
                handle_hotkey_event(
                    &app, &sm, hotkey_event, &mut push_to_talk_active,
                    &whisper, &sentence_tx, &llm_token_tx, &conversation,
                )
                .await;
            }
        }
    }
}

async fn handle_vad_event(
    app: &AppHandle,
    sm: &SharedStateMachine,
    event: VadEvent,
    whisper: &Arc<Mutex<WhisperEngine>>,
    sentence_tx: &mpsc::Sender<String>,
    llm_token_tx: &mpsc::Sender<String>,
    conversation: &Arc<Mutex<Conversation>>,
) {
    match event {
        VadEvent::SpeechStart => {
            let mut sm = sm.lock().unwrap();
            if sm.current() == RenState::Idle {
                let _ = sm.transition(RenState::Listening);
            }
        }
        VadEvent::SpeechEnd(audio) => {
            let current = sm.lock().unwrap().current();
            if current == RenState::Listening {
                sm.lock().unwrap().transition(RenState::Thinking).ok();
                run_full_turn(app, sm, &audio, whisper, sentence_tx, llm_token_tx, conversation)
                    .await;
            }
        }
    }
}

async fn handle_hotkey_event(
    app: &AppHandle,
    sm: &SharedStateMachine,
    event: HotkeyEvent,
    ptt_active: &mut bool,
    whisper: &Arc<Mutex<WhisperEngine>>,
    sentence_tx: &mpsc::Sender<String>,
    llm_token_tx: &mpsc::Sender<String>,
    conversation: &Arc<Mutex<Conversation>>,
) {
    match event {
        HotkeyEvent::PushToTalkStart => {
            if !*ptt_active {
                *ptt_active = true;
                let mut sm = sm.lock().unwrap();
                if matches!(sm.current(), RenState::Sleeping | RenState::Idle) {
                    let _ = sm.transition(RenState::Listening);
                }
            }
        }
        HotkeyEvent::PushToTalkEnd => {
            *ptt_active = false;
            // VAD SpeechEnd drives the actual transcription trigger
        }
        HotkeyEvent::ForceSleep => {
            sm.lock().unwrap().force(RenState::Sleeping);
        }
    }
}

// ─── Full pipeline turn: audio → STT → LLM → TTS ────────────────────────────

async fn run_full_turn(
    app: &AppHandle,
    sm: &SharedStateMachine,
    audio: &[f32],
    whisper: &Arc<Mutex<WhisperEngine>>,
    sentence_tx: &mpsc::Sender<String>,
    llm_token_tx: &mpsc::Sender<String>,
    conversation: &Arc<Mutex<Conversation>>,
) {
    // 1. STT
    let transcript = match run_stt(app, sm, audio, whisper).await {
        Some(t) => t,
        None => return,
    };

    // Emit transcript to frontend
    let _ = app.emit(
        "ren://transcript",
        TranscriptPayload { text: transcript.clone(), is_final: true },
    );

    // 2. LLM — only if Ollama is running
    if llm::ollama_process::active_port() == 0 {
        warn!("Ollama not running — skipping LLM turn, showing transcript only");
        sm.lock().unwrap().transition(RenState::Idle).ok();
        return;
    }

    let client = default_client();
    let sentence_tx = sentence_tx.clone();
    let llm_token_tx = llm_token_tx.clone();
    let conv = conversation.clone();
    let sm_clone = sm.clone();
    let app_clone = app.clone();

    tokio::spawn(async move {
        let result = {
            let mut conv = conv.lock().unwrap();
            // We must drop the lock before awaiting — clone messages out
            let user_text = transcript.clone();
            drop(conv);

            let mut conv = conversation.lock().unwrap();
            llm::run_turn(
                &client,
                &mut conv,
                &user_text,
                &llm_token_tx,
                &sentence_tx,
            )
            .await
        };

        match result {
            Ok(_) => {}
            Err(e) => {
                sm_clone
                    .lock()
                    .unwrap()
                    .emit_error("llm_failed", &e.to_string());
            }
        }
    });
}

async fn run_stt(
    app: &AppHandle,
    sm: &SharedStateMachine,
    audio: &[f32],
    whisper: &Arc<Mutex<WhisperEngine>>,
) -> Option<String> {
    // Lazy load
    {
        let mut engine = whisper.lock().unwrap();
        if !engine.is_loaded() {
            drop(engine);
            let w = whisper.clone();
            let load_result = tokio::task::spawn_blocking(move || {
                tokio::runtime::Handle::current()
                    .block_on(async { w.lock().unwrap().load().await })
            })
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!("{}", e)));

            if let Err(e) = load_result {
                sm.lock()
                    .unwrap()
                    .emit_error("model_load_failed", &e.to_string());
                return None;
            }
        }
    }

    let audio_owned = audio.to_vec();
    let w = whisper.clone();
    let result = tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current()
            .block_on(async { w.lock().unwrap().transcribe(&audio_owned).await })
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!("{}", e)));

    match result {
        Ok(text) if !text.trim().is_empty() => Some(text),
        Ok(_) => {
            sm.lock().unwrap().transition(RenState::Idle).ok();
            None
        }
        Err(e) => {
            sm.lock()
                .unwrap()
                .emit_error("transcription_failed", &e.to_string());
            None
        }
    }
}

// ─── TTS sentence consumer ────────────────────────────────────────────────────

async fn tts_sentence_loop(
    app: AppHandle,
    sm: SharedStateMachine,
    mut sentence_rx: mpsc::Receiver<String>,
    kokoro: Arc<Mutex<KokoroEngine>>,
    player: Arc<AudioPlayer>,
) {
    while let Some(sentence) = sentence_rx.recv().await {
        // Lazy load Kokoro
        {
            let mut engine = kokoro.lock().unwrap();
            if !engine.is_loaded() {
                drop(engine);
                let k = kokoro.clone();
                if let Err(e) = tokio::task::spawn_blocking(move || {
                    tokio::runtime::Handle::current()
                        .block_on(async { k.lock().unwrap().load().await })
                })
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!("{}", e)))
                {
                    sm.lock()
                        .unwrap()
                        .emit_error("tts_load_failed", &e.to_string());
                    continue;
                }
            }
        }

        sm.lock()
            .unwrap()
            .transition(RenState::Speaking)
            .ok();

        let sample_rate = kokoro.lock().unwrap().sample_rate();

        let audio = {
            let engine = kokoro.lock().unwrap();
            engine.synthesize(&sentence).await
        };

        match audio {
            Ok(buffer) => {
                if let Err(e) = player.play(&app, buffer, sample_rate).await {
                    warn!("Playback error: {}", e);
                }
            }
            Err(e) => {
                warn!("TTS synthesis failed: {} — skipping sentence", e);
            }
        }

        sm.lock().unwrap().transition(RenState::Idle).ok();
    }
}

// ─── Tauri helpers ────────────────────────────────────────────────────────────

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show_hide = MenuItem::with_id(app, "show_hide", "Show Ren", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_hide, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Ren")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_hide" => { let _ = commands::toggle_window(app.clone()); }
            "quit"      => { app.exit(0); }
            _           => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = commands::toggle_window(tray.app_handle().clone());
            }
        })
        .build(app)?;

    Ok(())
}

fn position_window_bottom_right(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("main") {
        if let Some(monitor) = window.current_monitor()? {
            let ms = monitor.size();
            let ws = window.outer_size()?;
            window.set_position(PhysicalPosition::new(
                ms.width as i32 - ws.width as i32 - 20,
                ms.height as i32 - ws.height as i32 - 60,
            ))?;
        }
    }
    Ok(())
}
