// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
mod config;
mod dismissal;
mod download;
mod hotkey;
mod llm;
mod playback;
mod state;
mod stt;
mod tools;
mod tts;
mod wake;

use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::time::{sleep, Instant};

/// Conversation is held across `.await` points during LLM streaming,
/// so it must use an async-aware mutex.
type SharedConversation = Arc<AsyncMutex<Conversation>>;

/// Whisper and Kokoro engines are driven from async tasks (load + transcribe
/// / synthesize are all async). Using `tokio::sync::Mutex` ensures the guard
/// is `Send` and can be held across `.await` safely.
type SharedWhisper = Arc<AsyncMutex<WhisperEngine>>;
type SharedKokoro = Arc<AsyncMutex<KokoroEngine>>;
use tracing::{info, warn};

use crate::{
    audio::{vad::VadEvent, WakeHookup},
    hotkey::HotkeyEvent,
    llm::{conversation::Conversation, default_client},
    playback::AudioPlayer,
    state::{RenState, SharedStateMachine},
    stt::{whisper::WhisperEngine, SttEngine},
    tools::ToolRegistry,
    tts::{kokoro::KokoroEngine, TtsEngine},
    wake::{porcupine::PorcupineWakeEngine, WakeEngine, WakeEvent, WakeKeyword},
};

fn build_tool_registry(config: &config::AppConfig) -> Arc<ToolRegistry> {
    use crate::tools::apps::AppLauncher;
    use crate::tools::files::OpenFolder;
    use crate::tools::steam::SteamLauncher;
    use crate::tools::system::{LockScreen, RestartSystem, ShutdownSystem, VolumeControl};
    use crate::tools::web::{default_client as web_client, Weather, WebSearch};

    let http = web_client();
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(VolumeControl));
    registry.register(Arc::new(LockScreen));
    registry.register(Arc::new(ShutdownSystem));
    registry.register(Arc::new(RestartSystem));
    registry.register(Arc::new(AppLauncher::new()));
    registry.register(Arc::new(SteamLauncher::new()));
    registry.register(Arc::new(OpenFolder));
    registry.register(Arc::new(Weather::new(http.clone(), config)));
    registry.register(Arc::new(WebSearch::new(http, config)));
    info!("Registered {} tools", registry.len());
    Arc::new(registry)
}

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

            let tool_registry = build_tool_registry(&config);

            setup_tray(app)?;
            position_window_bottom_right(&app_handle)?;

            let (vad_event_tx, vad_event_rx) = mpsc::channel::<VadEvent>(32);
            let (hotkey_event_tx, hotkey_event_rx) = mpsc::channel::<HotkeyEvent>(16);
            let (sentence_tx, sentence_rx) = mpsc::channel::<String>(32);
            let (llm_token_tx, _llm_token_rx) = mpsc::channel::<String>(256);
            let (wake_event_tx, wake_event_rx) = mpsc::channel::<WakeEvent>(8);

            // Optional wake-word engine. Returns `None` if the `wake` feature is
            // off, the Picovoice access key is missing, or the .ppn resources are
            // not bundled — in any of those cases the audio pipeline falls back
            // to hotkey-only activation.
            let wake_engine = build_wake_engine(&app_handle);
            let wake_hookup = wake_engine.clone().map(|engine| WakeHookup {
                engine,
                event_tx: wake_event_tx.clone(),
                state_machine: state_machine.clone(),
            });

            // Audio pipeline — leaked intentionally so the cpal Stream and VAD task
            // live for the entire process lifetime. Dropping the setup-scope handle
            // would silently stop capture.
            let audio_stream = audio::start_pipeline(vad_event_tx, wake_hookup)
                .expect("Failed to start audio pipeline");
            Box::leak(Box::new(audio_stream));

            // Hotkeys — same reasoning: manager drop would unregister every hotkey
            // and kill the listener task.
            let hotkey_manager = hotkey::start(hotkey_event_tx)
                .expect("Failed to register hotkeys");
            Box::leak(Box::new(hotkey_manager));

            // Engines (lazily loaded)
            let whisper: SharedWhisper = Arc::new(AsyncMutex::new(WhisperEngine::new()));
            let kokoro: SharedKokoro = Arc::new(AsyncMutex::new(KokoroEngine::new(
                Some(config.tts_voice.as_str()),
            )));

            // Audio player for TTS output
            let player = Arc::new(
                AudioPlayer::new().expect("Failed to initialize audio output"),
            );

            // Ollama child process (started async after setup)
            let ollama_child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
            let ollama_child_clone = ollama_child.clone();
            tauri::async_runtime::spawn(async move {
                match llm::ollama_process::start().await {
                    Ok(child) => {
                        *ollama_child_clone.lock().unwrap() = Some(child);
                        info!("Ollama child process ready");
                    }
                    Err(e) => {
                        warn!("Ollama not started: {} — LLM responses disabled", e);
                    }
                }
            });

            // TTS sentence consumer
            let kokoro_clone = kokoro.clone();
            let player_clone = player.clone();
            let sm_for_tts = state_machine.clone();
            let app_for_tts = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tts_sentence_loop(
                    app_for_tts,
                    sm_for_tts,
                    sentence_rx,
                    kokoro_clone,
                    player_clone,
                )
                .await;
            });

            // Conversation idle timer — Idle → Sleeping after configured timeout.
            spawn_conversation_timer(
                state_machine.clone(),
                Duration::from_secs(config.conversation_timeout_secs),
            );

            // Model lifecycle observer — unload heavy engines when entering Sleeping.
            spawn_model_unloader(
                state_machine.clone(),
                whisper.clone(),
                kokoro.clone(),
            );

            // Wake event observer — translate `ren://wake` signals into the
            // Sleeping → Waking → Listening state walk and the startup ack chime.
            if let Some(engine) = wake_engine.clone() {
                spawn_wake_event_observer(
                    state_machine.clone(),
                    wake_event_rx,
                    app_handle.clone(),
                );
                preload_wake_engine(engine);
            } else {
                // Keep the receiver alive so the channel never closes; the tx
                // side in the hookup would never fire but silencing unused warns.
                drop(wake_event_rx);
            }

            // Main event loop
            let sm = state_machine.clone();
            let handle = app_handle.clone();
            let conversation: SharedConversation = Arc::new(AsyncMutex::new(
                Conversation::new(Some(tool_registry.as_ref())),
            ));
            let registry_for_loop = tool_registry.clone();
            tauri::async_runtime::spawn(async move {
                event_loop(
                    handle,
                    sm,
                    vad_event_rx,
                    hotkey_event_rx,
                    whisper,
                    sentence_tx,
                    llm_token_tx,
                    conversation,
                    registry_for_loop,
                )
                .await;
            });

            // Transition to Sleeping once setup is complete
            state_machine
                .lock()
                .unwrap()
                .transition(RenState::Sleeping)
                .unwrap_or_else(|e| warn!("Init transition failed: {}", e));

            // Clean up Ollama on app exit (attach to main window)
            if let Some(window) = app.get_webview_window("main") {
                let ollama_on_exit = ollama_child.clone();
                window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::Destroyed) {
                        if let Some(child) = ollama_on_exit.lock().unwrap().as_mut() {
                            llm::ollama_process::terminate(child);
                        }
                    }
                });
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

// ─── State observers ─────────────────────────────────────────────────────────

/// Conversation mode: while in Idle, await follow-up. After `timeout` of
/// continuous Idle, automatically force-sleep so heavy models can be unloaded.
/// Any transition out of Idle cancels the pending sleep.
fn spawn_conversation_timer(sm: SharedStateMachine, timeout: Duration) {
    let mut rx = sm.lock().unwrap().subscribe();
    tauri::async_runtime::spawn(async move {
        let mut idle_since: Option<Instant> = None;

        loop {
            // Compute how long to wait until either a state change arrives or
            // the idle deadline expires.
            let wait = match idle_since {
                Some(start) => timeout
                    .checked_sub(start.elapsed())
                    .unwrap_or(Duration::from_millis(0)),
                None => Duration::from_secs(60 * 60), // effectively "until next event"
            };

            tokio::select! {
                changed = rx.recv() => match changed {
                    Ok(RenState::Idle) => idle_since = Some(Instant::now()),
                    Ok(_) => idle_since = None,
                    Err(_) => return, // channel closed → app shutting down
                },
                _ = sleep(wait), if idle_since.is_some() => {
                    if let Some(start) = idle_since {
                        if start.elapsed() >= timeout {
                            info!("Conversation idle timeout — returning to Sleeping");
                            sm.lock().unwrap().force(RenState::Sleeping);
                            idle_since = None;
                        }
                    }
                }
            }
        }
    });
}

/// On every Sleeping transition, drop the loaded STT and TTS models so they
/// release VRAM. Models are reloaded lazily on the next wake.
fn spawn_model_unloader(
    sm: SharedStateMachine,
    whisper: SharedWhisper,
    kokoro: SharedKokoro,
) {
    let mut rx = sm.lock().unwrap().subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(RenState::Sleeping) => {
                    let mut w = whisper.lock().await;
                    if w.is_loaded() {
                        info!("Sleeping — unloading Whisper");
                        w.unload();
                    }
                    drop(w);
                    let mut k = kokoro.lock().await;
                    if k.is_loaded() {
                        info!("Sleeping — unloading Kokoro");
                        k.unload();
                    }
                }
                Ok(_) => {}
                Err(_) => return,
            }
        }
    });
}

// ─── Wake word ───────────────────────────────────────────────────────────────

/// Resolves the Picovoice access key and the bundled `.ppn` resource files
/// and constructs a Porcupine engine. Returns `None` — with a warn log — if
/// any ingredient is missing so the rest of the app still boots into a
/// hotkey-only mode.
fn build_wake_engine(app: &AppHandle) -> Option<Arc<AsyncMutex<dyn WakeEngine>>> {
    use crate::config::defaults::{
        PICOVOICE_ACCESS_KEY, WAKE_KEYWORD_HEY_REN, WAKE_KEYWORD_REN_UYAN,
        WAKE_WORD_SENSITIVITY,
    };

    if PICOVOICE_ACCESS_KEY.is_empty() {
        warn!("PICOVOICE_ACCESS_KEY not set at build time — wake word disabled");
        return None;
    }

    let resource_dir = match app.path().resource_dir() {
        Ok(dir) => dir,
        Err(e) => {
            warn!("Could not resolve resource directory ({}); wake word disabled", e);
            return None;
        }
    };

    let wake_dir = resource_dir.join("wake");
    let keywords = [
        ("hey_ren", WAKE_KEYWORD_HEY_REN),
        ("ren_uyan", WAKE_KEYWORD_REN_UYAN),
    ];

    let mut resolved = Vec::with_capacity(keywords.len());
    for (id, filename) in keywords {
        let path = wake_dir.join(filename);
        if !path.exists() {
            warn!(
                "Wake keyword file missing: {} — wake word disabled",
                path.display()
            );
            return None;
        }
        let path_str = match path.to_str() {
            Some(s) => s.to_string(),
            None => {
                warn!(
                    "Wake keyword path contains invalid UTF-8: {}",
                    path.display()
                );
                return None;
            }
        };
        resolved.push(WakeKeyword {
            id: id.to_string(),
            model_path: path_str,
            sensitivity: WAKE_WORD_SENSITIVITY,
        });
    }

    let engine = PorcupineWakeEngine::new(PICOVOICE_ACCESS_KEY, resolved);
    Some(Arc::new(AsyncMutex::new(engine)) as Arc<AsyncMutex<dyn WakeEngine>>)
}

/// Loads the wake engine on a background task so startup stays responsive.
fn preload_wake_engine(engine: Arc<AsyncMutex<dyn WakeEngine>>) {
    tauri::async_runtime::spawn(async move {
        let mut guard = engine.lock().await;
        if let Err(e) = guard.load().await {
            warn!("Wake engine load failed: {}", e);
        }
    });
}

/// Walks Ren through `Sleeping → Waking → Listening` on wake detection.
/// Wake events that arrive in any other state are logged and ignored.
fn spawn_wake_event_observer(
    sm: SharedStateMachine,
    mut wake_rx: mpsc::Receiver<WakeEvent>,
    _app: AppHandle,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = wake_rx.recv().await {
            let current = sm.lock().unwrap().current();
            if !matches!(current, RenState::Sleeping) {
                info!(
                    "Wake '{}' ignored — current state {:?}",
                    event.keyword_id, current
                );
                continue;
            }

            info!("Wake '{}' accepted — transitioning Sleeping → Waking", event.keyword_id);
            {
                let mut m = sm.lock().unwrap();
                m.force(RenState::Waking);
            }

            // TODO(phase-4-home): decode `wake_ack.wav` from the bundled
            // resource dir and play it via `AudioPlayer`. Until the asset is
            // wired we rely on the orb animation for user feedback.

            {
                let mut m = sm.lock().unwrap();
                if let Err(e) = m.transition(RenState::Listening) {
                    warn!("Waking → Listening transition failed: {}", e);
                }
            }
        }
    });
}

// ─── Event loop ──────────────────────────────────────────────────────────────

async fn event_loop(
    app: AppHandle,
    sm: SharedStateMachine,
    mut vad_rx: mpsc::Receiver<VadEvent>,
    mut hotkey_rx: mpsc::Receiver<HotkeyEvent>,
    whisper: SharedWhisper,
    sentence_tx: mpsc::Sender<String>,
    llm_token_tx: mpsc::Sender<String>,
    conversation: SharedConversation,
    registry: Arc<ToolRegistry>,
) {
    let mut push_to_talk_active = false;

    loop {
        tokio::select! {
            Some(vad_event) = vad_rx.recv() => {
                handle_vad_event(
                    &app, &sm, vad_event, &whisper,
                    &sentence_tx, &llm_token_tx, &conversation, &registry,
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
    whisper: &SharedWhisper,
    sentence_tx: &mpsc::Sender<String>,
    llm_token_tx: &mpsc::Sender<String>,
    conversation: &SharedConversation,
    registry: &Arc<ToolRegistry>,
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
                run_full_turn(
                    app, sm, &audio, whisper,
                    sentence_tx, llm_token_tx, conversation, registry,
                )
                .await;
            }
        }
    }
}

async fn handle_hotkey_event(
    _app: &AppHandle,
    sm: &SharedStateMachine,
    event: HotkeyEvent,
    ptt_active: &mut bool,
    _whisper: &SharedWhisper,
    _sentence_tx: &mpsc::Sender<String>,
    _llm_token_tx: &mpsc::Sender<String>,
    _conversation: &SharedConversation,
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
            // The actual transcription trigger comes from VAD SpeechEnd.
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
    whisper: &SharedWhisper,
    sentence_tx: &mpsc::Sender<String>,
    llm_token_tx: &mpsc::Sender<String>,
    conversation: &SharedConversation,
    registry: &Arc<ToolRegistry>,
) {
    // 1. STT
    let transcript = match run_stt(sm, audio, whisper).await {
        Some(t) => t,
        None => return,
    };

    let _ = app.emit(
        "ren://transcript",
        TranscriptPayload {
            text: transcript.clone(),
            is_final: true,
        },
    );

    // Voice dismissal — short-circuit straight to Sleeping.
    if dismissal::is_dismissal(&transcript) {
        info!("Dismissal phrase detected — returning to Sleeping");
        sm.lock().unwrap().force(RenState::Sleeping);
        return;
    }

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
    let registry = registry.clone();
    let app_clone = app.clone();

    tauri::async_runtime::spawn(async move {
        let mut conv = conv.lock().await;
        let result = llm::run_turn(
            &app_clone,
            &client,
            registry,
            &mut conv,
            &transcript,
            &llm_token_tx,
            &sentence_tx,
        )
        .await;

        if let Err(e) = result {
            sm_clone
                .lock()
                .unwrap()
                .emit_error("llm_failed", &e.to_string());
        }
    });
}

async fn run_stt(
    sm: &SharedStateMachine,
    audio: &[f32],
    whisper: &SharedWhisper,
) -> Option<String> {
    // Lazy load
    {
        let mut guard = whisper.lock().await;
        if !guard.is_loaded() {
            if let Err(e) = guard.load().await {
                drop(guard);
                sm.lock()
                    .unwrap()
                    .emit_error("model_load_failed", &e.to_string());
                return None;
            }
        }
    }

    let result = {
        let mut guard = whisper.lock().await;
        guard.transcribe(audio).await
    };

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
    kokoro: SharedKokoro,
    player: Arc<AudioPlayer>,
) {
    while let Some(sentence) = sentence_rx.recv().await {
        // Lazy load Kokoro
        {
            let mut guard = kokoro.lock().await;
            if !guard.is_loaded() {
                if let Err(e) = guard.load().await {
                    drop(guard);
                    sm.lock()
                        .unwrap()
                        .emit_error("tts_load_failed", &e.to_string());
                    continue;
                }
            }
        }

        sm.lock().unwrap().transition(RenState::Speaking).ok();

        let (sample_rate, audio) = {
            let engine = kokoro.lock().await;
            let sr = engine.sample_rate();
            let a = engine.synthesize(&sentence).await;
            (sr, a)
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
