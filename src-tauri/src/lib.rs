// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod clipboard;
mod commands;
mod config;
mod dismissal;
mod download;
mod hotkey;
mod llm;
mod memory;
mod playback;
mod state;
mod storage;
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

fn build_tool_registry(
    config: &config::AppConfig,
    timer_registry: tools::remind::SharedTimerRegistry,
    reminder_store: tools::remind::SharedReminderStore,
) -> Arc<ToolRegistry> {
    use crate::tools::apps::AppLauncher;
    use crate::tools::files::{ListDir, OpenFolder, ReadText};
    use crate::tools::media::{
        MediaCurrentTrack, MediaNext, MediaPause, MediaPlay, MediaPrevious,
    };
    use crate::tools::memory::{Forget, Remember};
    use crate::tools::remind::{
        ReminderCancel, ReminderList, ReminderSet, TimerCancel, TimerList, TimerStart,
    };
    use crate::tools::steam::SteamLauncher;
    use crate::tools::system::{
        ActiveWindow, LockScreen, ResourceUsage, RestartSystem, RunningApps, ShutdownSystem,
        VolumeControl,
    };
    use crate::tools::time::{TimeNow, TimeUntil};
    use crate::tools::weather::Weather;
    use crate::tools::web::{default_client as web_client, WebSearch};

    let http = web_client();
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(VolumeControl));
    registry.register(Arc::new(LockScreen));
    registry.register(Arc::new(ShutdownSystem));
    registry.register(Arc::new(RestartSystem));
    registry.register(Arc::new(ActiveWindow));
    registry.register(Arc::new(ResourceUsage));
    registry.register(Arc::new(RunningApps));
    registry.register(Arc::new(AppLauncher::new()));
    registry.register(Arc::new(SteamLauncher::new()));
    registry.register(Arc::new(OpenFolder));
    registry.register(Arc::new(ListDir));
    registry.register(Arc::new(ReadText));
    registry.register(Arc::new(MediaPlay));
    registry.register(Arc::new(MediaPause));
    registry.register(Arc::new(MediaNext));
    registry.register(Arc::new(MediaPrevious));
    registry.register(Arc::new(MediaCurrentTrack));
    registry.register(Arc::new(TimeNow));
    registry.register(Arc::new(TimeUntil));
    registry.register(Arc::new(Remember));
    registry.register(Arc::new(Forget));
    registry.register(Arc::new(TimerStart { registry: timer_registry.clone() }));
    registry.register(Arc::new(TimerList  { registry: timer_registry.clone() }));
    registry.register(Arc::new(TimerCancel { registry: timer_registry }));
    registry.register(Arc::new(ReminderSet    { store: reminder_store.clone() }));
    registry.register(Arc::new(ReminderList   { store: reminder_store.clone() }));
    registry.register(Arc::new(ReminderCancel { store: reminder_store }));
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
            let shared_config: config::SharedConfig =
                Arc::new(Mutex::new(config.clone()));
            app.manage(shared_config.clone());

            // Clipboard "armed context" — set by Ctrl+Shift+Alt+V and consumed
            // (cleared) on the next user turn.
            let clipboard_arm = clipboard::new_arm();
            app.manage(clipboard_arm.clone());

            // One-shot archive prune at startup. Best-effort; logs on failure.
            if config.memory_enabled {
                let retention = config.memory_archive_retention_days;
                tauri::async_runtime::spawn_blocking(move || {
                    if let Ok(store) = memory::MemoryStore::open() {
                        store.prune_archive(retention);
                    }
                });
            }

            let state_machine = state::new_shared(app_handle.clone());
            app.manage(state_machine.clone());

            // Proactive scheduling — shared fire channel feeds both timers
            // (in-memory) and reminders (persisted). `spawn_reminder_fire_loop`
            // bridges firings into speech + frontend events.
            let (fire_tx, fire_rx) = tools::remind::fire_channel();
            let timer_registry = tools::remind::TimerRegistry::new(fire_tx.clone());
            let reminder_store = tools::remind::ReminderStore::open(fire_tx.clone())
                .expect("Failed to open reminder store");
            tools::remind::reminder::spawn_poll_loop(reminder_store.clone());

            let tool_registry =
                build_tool_registry(&config, timer_registry.clone(), reminder_store.clone());

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

            // ren-tts sidecar — owns ORT/CUDA in its own process so it cannot
            // collide with whisper.cpp's CUDA backend living in this one.
            let tts_child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
            let tts_child_clone = tts_child.clone();
            let tts_voice_for_spawn = config.tts_voice.clone();
            tauri::async_runtime::spawn(async move {
                match tts::process::start(&tts_voice_for_spawn).await {
                    Ok(child) => {
                        *tts_child_clone.lock().unwrap() = Some(child);
                        info!("ren-tts sidecar ready");
                    }
                    Err(e) => {
                        warn!("ren-tts not started: {} — TTS disabled", e);
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

            // Proactive-alert consumer — timers and reminders land here and
            // get narrated through the existing TTS sentence pipeline.
            spawn_reminder_fire_loop(
                app_handle.clone(),
                state_machine.clone(),
                fire_rx,
                sentence_tx.clone(),
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
                // Force WebView2 to use a transparent backbuffer so the
                // window is truly see-through on Windows.
                let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
                let ollama_on_exit = ollama_child.clone();
                let tts_on_exit = tts_child.clone();
                window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::Destroyed) {
                        if let Some(child) = ollama_on_exit.lock().unwrap().as_mut() {
                            llm::ollama_process::terminate(child);
                        }
                        if let Some(child) = tts_on_exit.lock().unwrap().as_mut() {
                            tts::process::terminate(child);
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
            commands::get_config,
            commands::save_config,
            commands::open_settings,
            commands::clear_clipboard_arm,
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

// ─── Proactive alerts ─────────────────────────────────────────────────────────

/// Bridges `Firing`s from timer / reminder systems into a spoken narration
/// and a `ren://reminder` frontend event. If Ren was Sleeping we nudge it
/// awake first so the TTS sentence pipeline actually plays the alert.
fn spawn_reminder_fire_loop(
    app: AppHandle,
    sm: SharedStateMachine,
    mut fire_rx: tools::remind::FireReceiver,
    sentence_tx: mpsc::Sender<String>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(firing) = fire_rx.recv().await {
            let sentence = match firing.kind {
                "timer" => format!("Your timer for {} is up, sir.", firing.label),
                "reminder" => format!("Reminder: {}.", firing.label),
                _ => format!("Alert: {}.", firing.label),
            };

            {
                let mut guard = sm.lock().unwrap();
                if matches!(guard.current(), RenState::Sleeping) {
                    guard.force(RenState::Waking);
                }
            }

            let _ = app.emit(
                "ren://reminder",
                serde_json::json!({
                    "kind": firing.kind,
                    "label": firing.label,
                }),
            );

            if sentence_tx.send(sentence).await.is_err() {
                warn!("Reminder sentence channel closed — stopping fire loop");
                return;
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
    app: &AppHandle,
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
        HotkeyEvent::ArmClipboardContext => {
            arm_clipboard_context(app).await;
        }
    }
}

/// Reads the Windows clipboard on a blocking thread, stores it in the
/// shared "armed" slot, and tells the frontend to surface a badge. Errors
/// are logged but never crash the hotkey loop — the user can simply press
/// the chord again.
async fn arm_clipboard_context(app: &AppHandle) {
    let read_result =
        tokio::task::spawn_blocking(crate::clipboard::read_text).await;
    let text = match read_result {
        Ok(Ok(t)) if !t.trim().is_empty() => t,
        Ok(Ok(_)) => {
            warn!("Clipboard is empty — nothing to arm");
            return;
        }
        Ok(Err(e)) => {
            warn!("Clipboard read failed: {}", e);
            return;
        }
        Err(e) => {
            warn!("Clipboard read task panicked: {}", e);
            return;
        }
    };

    let preview = crate::clipboard::preview_of(&text);
    if let Some(arm) = app.try_state::<crate::clipboard::SharedClipboardArm>() {
        *arm.lock().unwrap() = Some(text);
    } else {
        warn!("Clipboard arm state not registered — skipping");
        return;
    }

    let _ = app.emit(
        "ren://clipboard-armed",
        serde_json::json!({ "preview": preview }),
    );
    info!("Clipboard context armed ({} chars preview)", preview.chars().count());
}

/// Drains the shared "armed clipboard" slot. If something is armed, returns
/// the user transcript wrapped with the captured preamble so the LLM can
/// clearly see both. Always emits `ren://clipboard-armed` with a `null`
/// preview so the frontend badge clears.
fn consume_clipboard_arm(app: &AppHandle, transcript: String) -> String {
    let armed = match app.try_state::<crate::clipboard::SharedClipboardArm>() {
        Some(state) => state.lock().unwrap().take(),
        None => None,
    };

    let _ = app.emit(
        "ren://clipboard-armed",
        serde_json::json!({ "preview": serde_json::Value::Null }),
    );

    match armed {
        Some(clip) => format!(
            "[Clipboard context]\n{}\n\n[User said]\n{}",
            clip, transcript
        ),
        None => transcript,
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

    // Voice dismissal — short-circuit straight to Sleeping. Run on the raw
    // transcript before any clipboard preamble is wrapped around it, so the
    // dismissal substring match still hits.
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

    // Archive the *raw* user transcript before any clipboard wrap so we
    // never write pasted content (potentially sensitive) to long-term
    // storage. Best-effort — internal logging on failure.
    archive_user_turn(&transcript);

    // Drain any armed clipboard preamble — clears the badge on the frontend
    // either way (so the user is never surprised by stale context).
    let transcript = consume_clipboard_arm(app, transcript);

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

        match result {
            Ok(reply) if !reply.trim().is_empty() => {
                archive_assistant_turn(&reply);
            }
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

fn archive_user_turn(text: &str) {
    let text = text.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(store) = crate::memory::MemoryStore::open() {
            store.archive_turn(crate::memory::ArchiveRole::User, &text);
        }
    });
}

fn archive_assistant_turn(text: &str) {
    let text = text.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(store) = crate::memory::MemoryStore::open() {
            store.archive_turn(crate::memory::ArchiveRole::Assistant, &text);
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
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_hide, &settings, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Ren")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_hide" => { let _ = commands::toggle_window(app.clone()); }
            "settings"  => { let _ = commands::open_settings(app.clone()); }
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
