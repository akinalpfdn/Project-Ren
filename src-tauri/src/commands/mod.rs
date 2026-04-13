use tauri::{AppHandle, Manager};
use tracing::info;

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
