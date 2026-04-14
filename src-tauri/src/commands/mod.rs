use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::clipboard::SharedClipboardArm;
use crate::config::{AppConfig, SharedConfig};
use crate::state::{RenState, SharedStateMachine};

/// Toggle window visibility.
#[tauri::command]
pub fn toggle_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().map_err(|e| e.to_string())? {
            window.hide().map_err(|e| e.to_string())?;
        } else {
            window.show().map_err(|e| e.to_string())?;
            window.set_focus().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Show the main window.
#[tauri::command]
pub fn show_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Hide the main window.
#[tauri::command]
pub fn hide_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Expose current Ren state to the frontend on demand.
/// The frontend normally tracks state via events, but can call this on init.
#[tauri::command]
pub fn get_state(
    state_machine: tauri::State<'_, SharedStateMachine>,
) -> String {
    let sm = state_machine.lock().unwrap();
    serde_json::to_string(&sm.current()).unwrap_or_else(|_| "\"sleeping\"".to_string())
}

/// Returns a snapshot of the current persisted settings so the Settings
/// panel can hydrate its form without duplicating defaults in TypeScript.
#[tauri::command]
pub fn get_config(config: tauri::State<'_, SharedConfig>) -> Result<AppConfig, String> {
    Ok(config.lock().unwrap().clone())
}

/// Accepts a full `AppConfig` from the Settings panel, replaces the
/// in-memory snapshot, and persists to `%APPDATA%\Ren\config.json`.
///
/// The handler returns only after the write hits disk so the UI can render
/// an honest "saved" state — a tauri event is also fired so any other
/// subsystems that care about config changes can subscribe.
#[tauri::command]
pub fn save_config(
    app: AppHandle,
    config_state: tauri::State<'_, SharedConfig>,
    new_config: AppConfig,
) -> Result<(), String> {
    new_config.save().map_err(|e| {
        warn!("Failed to persist config: {}", e);
        e.to_string()
    })?;
    {
        let mut guard = config_state.lock().unwrap();
        *guard = new_config;
    }
    info!("Config saved via settings panel");
    let _ = app.emit("ren://config-saved", ());
    Ok(())
}

/// Drops any armed clipboard preamble. Bound to ESC in the frontend so the
/// user can change their mind before submitting the next turn.
#[tauri::command]
pub fn clear_clipboard_arm(
    app: AppHandle,
    arm: tauri::State<'_, SharedClipboardArm>,
) -> Result<(), String> {
    *arm.lock().unwrap() = None;
    let _ = app.emit(
        "ren://clipboard-armed",
        serde_json::json!({ "preview": serde_json::Value::Null }),
    );
    Ok(())
}

/// Tray menu "Settings" handler — tells the frontend to reveal the panel.
/// The window is forced visible first so the event lands on a rendered view.
#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    app.emit("ren://open-settings", ())
        .map_err(|e| e.to_string())
}
