//! `system.lock` — lock the Windows workstation via `LockWorkStation`.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolError, ToolResult};

pub struct LockScreen;

#[async_trait]
impl Tool for LockScreen {
    fn name(&self) -> &str {
        "system.lock"
    }

    fn description(&self) -> &str {
        "Lock the workstation, showing the Windows sign-in screen. No parameters."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        lock_workstation().map_err(|e| ToolError::execution(self.name(), e))?;
        Ok(ToolResult::new("Workstation locked."))
    }
}

#[cfg(windows)]
fn lock_workstation() -> Result<(), String> {
    use windows::Win32::System::Shutdown::LockWorkStation;
    // SAFETY: FFI call into documented Win32 API with no pointer arguments.
    // Returns BOOL; non-zero is success.
    let ok = unsafe { LockWorkStation() };
    if ok.is_ok() {
        Ok(())
    } else {
        Err("LockWorkStation returned failure".into())
    }
}

#[cfg(not(windows))]
fn lock_workstation() -> Result<(), String> {
    Err("workstation lock is only supported on Windows".into())
}
