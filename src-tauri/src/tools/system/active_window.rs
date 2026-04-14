//! `system.active_window` — answers "what is the user looking at right now"
//! by querying the Win32 foreground window: window title + owning process
//! basename. Useful for context-aware prompts ("explain what's on screen",
//! "remind me what app this was").

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolError, ToolResult};

pub struct ActiveWindow;

#[async_trait]
impl Tool for ActiveWindow {
    fn name(&self) -> &str {
        "system.active_window"
    }

    fn description(&self) -> &str {
        "Return the title and process name of the window the user is currently focused on. \
         Use this when the user asks 'what am I doing' or 'what app is this'."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        let info = tokio::task::spawn_blocking(read_active_window)
            .await
            .map_err(|e| ToolError::execution(self.name(), format!("join error: {}", e)))?
            .map_err(|e| ToolError::execution(self.name(), e))?;

        let summary = match (info.title.is_empty(), info.process.is_empty()) {
            (true, true) => "There is no foreground window right now.".to_string(),
            (false, true) => format!("Foreground window: \"{}\".", info.title),
            (true, false) => format!("Foreground process: {}.", info.process),
            (false, false) => format!("\"{}\" — {}.", info.title, info.process),
        };
        Ok(ToolResult::with_detail(
            summary,
            json!({ "title": info.title, "process": info.process }).to_string(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct WindowInfo {
    pub title: String,
    pub process: String,
}

#[cfg(windows)]
pub fn read_active_window() -> Result<WindowInfo, String> {
    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return Ok(WindowInfo::default());
        }

        // Title
        let title_len = GetWindowTextLengthW(hwnd);
        let title = if title_len > 0 {
            let mut buf = vec![0u16; title_len as usize + 1];
            let copied = GetWindowTextW(hwnd, &mut buf);
            String::from_utf16_lossy(&buf[..copied as usize])
        } else {
            String::new()
        };

        // Process
        let mut pid: u32 = 0;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut process = String::new();
        if pid != 0 {
            if let Ok(handle) = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                false,
                pid,
            ) {
                let mut name_buf = vec![0u16; MAX_PATH as usize];
                let copied = GetModuleBaseNameW(handle, None, &mut name_buf);
                if copied > 0 {
                    process = String::from_utf16_lossy(&name_buf[..copied as usize]);
                }
                let _ = CloseHandle(handle);
            }
        }

        Ok(WindowInfo { title, process })
    }
}

#[cfg(not(windows))]
pub fn read_active_window() -> Result<WindowInfo, String> {
    Err("active_window is only supported on Windows".to_string())
}
