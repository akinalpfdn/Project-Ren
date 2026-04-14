//! `system.running_apps` — list visible top-level windows the user could
//! reasonably switch to. Multiple windows of the same process collapse into
//! one entry (e.g. five Chrome windows → one "chrome.exe").

use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolError, ToolResult};

const MAX_APPS: usize = 30;

pub struct RunningApps;

#[async_trait]
impl Tool for RunningApps {
    fn name(&self) -> &str {
        "system.running_apps"
    }

    fn description(&self) -> &str {
        "List the user's currently visible apps. Useful for 'what's open' \
         or 'switch to X' style prompts. Multiple windows from the same app \
         are collapsed into a single entry."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        let mut apps = tokio::task::spawn_blocking(enumerate_visible_windows)
            .await
            .map_err(|e| ToolError::execution(self.name(), format!("join error: {}", e)))?
            .map_err(|e| ToolError::execution(self.name(), e))?;

        apps.sort_by(|a, b| a.process.to_lowercase().cmp(&b.process.to_lowercase()));
        apps.truncate(MAX_APPS);

        if apps.is_empty() {
            return Ok(ToolResult::new("No visible top-level windows right now.".to_string()));
        }

        let summary = format!(
            "{} visible apps: {}.",
            apps.len(),
            apps.iter()
                .map(|a| a.process.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let detail = apps
            .iter()
            .map(|a| format!("{:<25} {}", a.process, a.example_title))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolResult::with_detail(summary, detail))
    }
}

#[derive(Debug, Clone)]
struct AppEntry {
    process: String,
    example_title: String,
}

#[cfg(windows)]
fn enumerate_visible_windows() -> Result<Vec<AppEntry>, String> {
    use std::collections::HashMap;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, MAX_PATH};
    use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible,
    };

    // EnumWindows is a sync C callback; collect into a static-style Mutex.
    let collected: Mutex<HashMap<String, AppEntry>> = Mutex::new(HashMap::new());

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let collected =
            unsafe { &*(lparam.0 as *const Mutex<std::collections::HashMap<String, AppEntry>>) };

        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        if IsIconic(hwnd).as_bool() {
            return BOOL(1);
        }

        let title_len = GetWindowTextLengthW(hwnd);
        if title_len <= 0 {
            return BOOL(1);
        }
        let mut title_buf = vec![0u16; title_len as usize + 1];
        let copied = GetWindowTextW(hwnd, &mut title_buf);
        let title = String::from_utf16_lossy(&title_buf[..copied as usize]);
        if title.trim().is_empty() {
            return BOOL(1);
        }

        let mut pid: u32 = 0;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return BOOL(1);
        }

        let process = match OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            false,
            pid,
        ) {
            Ok(handle) => {
                let mut name_buf = vec![0u16; MAX_PATH as usize];
                let copied = GetModuleBaseNameW(handle, None, &mut name_buf);
                let name = if copied > 0 {
                    String::from_utf16_lossy(&name_buf[..copied as usize])
                } else {
                    String::new()
                };
                let _ = windows::Win32::Foundation::CloseHandle(handle);
                name
            }
            Err(_) => String::new(),
        };
        if process.trim().is_empty() {
            return BOOL(1);
        }

        let key = process.to_lowercase();
        let mut map = collected.lock().unwrap();
        map.entry(key).or_insert(AppEntry {
            process,
            example_title: title,
        });
        BOOL(1)
    }

    let lparam = LPARAM(&collected as *const _ as isize);
    unsafe {
        EnumWindows(Some(enum_proc), lparam)
            .map_err(|e| format!("EnumWindows failed: {}", e))?;
    }
    Ok(collected.into_inner().unwrap().into_values().collect())
}

#[cfg(not(windows))]
fn enumerate_visible_windows() -> Result<Vec<AppEntry>, String> {
    Err("running_apps is only supported on Windows".to_string())
}
