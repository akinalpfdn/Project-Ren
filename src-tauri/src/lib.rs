// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
mod config;
mod download;
mod hotkey;
mod state;
mod stt;

use std::sync::{Arc, Mutex};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tokio::sync::mpsc;
use tracing::info;

use crate::{
    audio::vad::VadEvent,
    hotkey::HotkeyEvent,
    stt::whisper::WhisperEngine,
    state::{RenState, SharedStateMachine},
};

/// Payload for transcript events sent to the frontend.
#[derive(Clone, serde::Serialize)]
struct TranscriptPayload {
    text: String,
    is_final: bool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize structured logging
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

            // Load config
            let config = config::AppConfig::load()
                .unwrap_or_else(|e| {
                    tracing::warn!("Config load failed ({}), using defaults", e);
                    config::AppConfig::default()
                });

            // State machine
            let state_machine = state::new_shared(app_handle.clone());
            app.manage(state_machine.clone());

            // System tray
            setup_tray(app)?;

            // Position window at bottom-right
            position_window_bottom_right(&app_handle)?;

            // Channels
            let (vad_event_tx, vad_event_rx) = mpsc::channel::<VadEvent>(32);
            let (hotkey_event_tx, hotkey_event_rx) = mpsc::channel::<HotkeyEvent>(16);

            // Audio pipeline
            let _audio_stream = audio::start_pipeline(vad_event_tx)
                .unwrap_or_else(|e| {
                    tracing::error!("Audio pipeline failed to start: {}", e);
                    // Emit error to frontend
                    let _ = app_handle.emit(
                        "ren://error",
                        state::ErrorPayload {
                            code: "audio_init_failed".to_string(),
                            message: e.to_string(),
                        },
                    );
                    // Return a stub — we still want the UI to work
                    panic!("Cannot start without audio: {}", e);
                });

            // Hotkeys
            let _hotkey_manager = hotkey::start(hotkey_event_tx)
                .unwrap_or_else(|e| {
                    tracing::warn!("Hotkey registration failed: {}", e);
                    panic!("Cannot register hotkeys: {}", e);
                });

            // STT engine (lazily loaded)
            let whisper = Arc::new(Mutex::new(WhisperEngine::new()));

            // Spawn main event loop
            let sm = state_machine.clone();
            let handle = app_handle.clone();
            tokio::spawn(async move {
                event_loop(handle, sm, vad_event_rx, hotkey_event_rx, whisper).await;
            });

            // Move to Sleeping after init
            {
                let mut sm = state_machine.lock().unwrap();
                sm.transition(RenState::Sleeping)
                    .unwrap_or_else(|e| tracing::warn!("Init transition failed: {}", e));
            }

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

/// Main event loop: routes VAD and hotkey events through the state machine.
async fn event_loop(
    app: AppHandle,
    sm: SharedStateMachine,
    mut vad_rx: mpsc::Receiver<VadEvent>,
    mut hotkey_rx: mpsc::Receiver<HotkeyEvent>,
    whisper: Arc<Mutex<WhisperEngine>>,
) {
    let mut push_to_talk_active = false;

    loop {
        tokio::select! {
            Some(vad_event) = vad_rx.recv() => {
                handle_vad_event(&app, &sm, vad_event, &whisper).await;
            }
            Some(hotkey_event) = hotkey_rx.recv() => {
                handle_hotkey_event(
                    &app, &sm, hotkey_event,
                    &mut push_to_talk_active,
                    &whisper,
                ).await;
            }
        }
    }
}

async fn handle_vad_event(
    app: &AppHandle,
    sm: &SharedStateMachine,
    event: VadEvent,
    whisper: &Arc<Mutex<WhisperEngine>>,
) {
    match event {
        VadEvent::SpeechStart => {
            let mut sm = sm.lock().unwrap();
            let current = sm.current();
            if current == RenState::Idle {
                let _ = sm.transition(RenState::Listening);
            }
        }
        VadEvent::SpeechEnd(audio) => {
            let current = {
                let sm = sm.lock().unwrap();
                sm.current()
            };
            if current == RenState::Listening {
                {
                    let mut sm = sm.lock().unwrap();
                    let _ = sm.transition(RenState::Thinking);
                }
                run_transcription(app, sm, &audio, whisper).await;
            }
        }
    }
}

async fn handle_hotkey_event(
    app: &AppHandle,
    sm: &SharedStateMachine,
    event: HotkeyEvent,
    push_to_talk_active: &mut bool,
    whisper: &Arc<Mutex<WhisperEngine>>,
) {
    match event {
        HotkeyEvent::PushToTalkStart => {
            if !*push_to_talk_active {
                *push_to_talk_active = true;
                let mut sm = sm.lock().unwrap();
                let current = sm.current();
                if matches!(current, RenState::Sleeping | RenState::Idle) {
                    let _ = sm.transition(RenState::Listening);
                }
            }
        }
        HotkeyEvent::PushToTalkEnd => {
            if *push_to_talk_active {
                *push_to_talk_active = false;
                // VAD SpeechEnd will handle the transition when it detects silence
            }
        }
        HotkeyEvent::ForceSleep => {
            let mut sm = sm.lock().unwrap();
            sm.force(RenState::Sleeping);
        }
    }
}

async fn run_transcription(
    app: &AppHandle,
    sm: &SharedStateMachine,
    audio: &[f32],
    whisper: &Arc<Mutex<WhisperEngine>>,
) {
    // Load Whisper lazily
    let load_result = {
        let mut engine = whisper.lock().unwrap();
        if !engine.is_loaded() {
            // We need to unlock before awaiting — clone the Arc for the async context
            drop(engine);
            let w = whisper.clone();
            tokio::task::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let mut engine = w.lock().unwrap();
                    engine.load().await
                })
            })
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!("Load task panicked: {}", e)))
        } else {
            Ok(())
        }
    };

    if let Err(e) = load_result {
        let mut sm = sm.lock().unwrap();
        sm.emit_error("model_load_failed", &e.to_string());
        return;
    }

    // Run transcription
    let audio_owned = audio.to_vec();
    let whisper_clone = whisper.clone();
    let result = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let engine = whisper_clone.lock().unwrap();
            engine.transcribe(&audio_owned).await
        })
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!("Transcription task panicked: {}", e)));

    match result {
        Ok(text) if !text.is_empty() => {
            let _ = app.emit(
                "ren://transcript",
                TranscriptPayload {
                    text,
                    is_final: true,
                },
            );
            // Transition to Idle (Phase 3 will add LLM → Speaking here)
            let mut sm = sm.lock().unwrap();
            let _ = sm.transition(RenState::Idle);
        }
        Ok(_) => {
            // Empty transcript — go back to listening or idle
            let mut sm = sm.lock().unwrap();
            let _ = sm.transition(RenState::Idle);
        }
        Err(e) => {
            let mut sm = sm.lock().unwrap();
            sm.emit_error("transcription_failed", &e.to_string());
        }
    }
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show_hide = MenuItem::with_id(app, "show_hide", "Show Ren", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_hide, &quit])?;

    let _tray = TrayIconBuilder::new()
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
            let monitor_size = monitor.size();
            let window_size = window.outer_size()?;
            let x = monitor_size.width as i32 - window_size.width as i32 - 20;
            let y = monitor_size.height as i32 - window_size.height as i32 - 60;
            window.set_position(PhysicalPosition::new(x, y))?;
        }
    }
    Ok(())
}
