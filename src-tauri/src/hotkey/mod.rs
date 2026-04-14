use anyhow::{Context, Result};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Events emitted by the hotkey listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// Ctrl+Shift+Alt+R pressed — start push-to-talk recording.
    PushToTalkStart,
    /// Ctrl+Shift+Alt+R released — stop recording, run transcription.
    PushToTalkEnd,
    /// Ctrl+Shift+Alt+S — force Ren back to Sleeping state.
    ForceSleep,
    /// Ctrl+Shift+Alt+V — capture the clipboard and arm it as context for
    /// the next user turn.
    ArmClipboardContext,
}

/// Registers global hotkeys and forwards events to the returned receiver.
/// The `GlobalHotKeyManager` must be kept alive for hotkeys to remain registered.
pub fn start(event_tx: mpsc::Sender<HotkeyEvent>) -> Result<GlobalHotKeyManager> {
    let manager = GlobalHotKeyManager::new()
        .context("Failed to initialize global hotkey manager")?;

    let triple_mod = Modifiers::CONTROL | Modifiers::SHIFT | Modifiers::ALT;

    // Ctrl+Shift+Alt+R — push-to-talk
    let ptr_hotkey = HotKey::new(Some(triple_mod), Code::KeyR);

    // Ctrl+Shift+Alt+S — force sleep
    let sleep_hotkey = HotKey::new(Some(triple_mod), Code::KeyS);

    // Ctrl+Shift+Alt+V — arm clipboard context for the next turn.
    let clipboard_hotkey = HotKey::new(Some(triple_mod), Code::KeyV);

    let ptr_id = ptr_hotkey.id();
    let sleep_id = sleep_hotkey.id();
    let clipboard_id = clipboard_hotkey.id();

    manager
        .register(ptr_hotkey)
        .context("Failed to register Ctrl+Shift+Alt+R hotkey")?;
    manager
        .register(sleep_hotkey)
        .context("Failed to register Ctrl+Shift+Alt+S hotkey")?;
    manager
        .register(clipboard_hotkey)
        .context("Failed to register Ctrl+Shift+Alt+V hotkey")?;

    info!(
        "Hotkeys registered: Ctrl+Shift+Alt+R (push-to-talk), Ctrl+Shift+Alt+S (force sleep), Ctrl+Shift+Alt+V (clipboard context)"
    );

    // Spawn listener task on Tauri's async runtime (called from sync setup closure).
    tauri::async_runtime::spawn(async move {
        let receiver = GlobalHotKeyEvent::receiver();
        loop {
            if let Ok(event) = receiver.try_recv() {
                let hotkey_event = if event.id == ptr_id {
                    match event.state() {
                        global_hotkey::HotKeyState::Pressed  => Some(HotkeyEvent::PushToTalkStart),
                        global_hotkey::HotKeyState::Released => Some(HotkeyEvent::PushToTalkEnd),
                    }
                } else if event.id == sleep_id
                    && event.state() == global_hotkey::HotKeyState::Pressed
                {
                    Some(HotkeyEvent::ForceSleep)
                } else if event.id == clipboard_id
                    && event.state() == global_hotkey::HotKeyState::Pressed
                {
                    Some(HotkeyEvent::ArmClipboardContext)
                } else {
                    None
                };

                if let Some(ev) = hotkey_event {
                    if let Err(e) = event_tx.send(ev).await {
                        warn!("Hotkey event channel closed: {}", e);
                        break;
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    });

    Ok(manager)
}
