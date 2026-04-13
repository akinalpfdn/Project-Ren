//! `system.shutdown` — schedule an OS shutdown via `shutdown.exe`.
//!
//! Uses the built-in `shutdown.exe /s /t <seconds>` rather than calling
//! `ExitWindowsEx` directly — the shell command already handles the
//! privilege token ritual (`SeShutdownPrivilege`) and gives us a free
//! cancel path via `shutdown.exe /a`.

use std::process::Command;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolError, ToolResult, ToolSafety};

const DEFAULT_DELAY_SECS: u32 = 10;
const MAX_DELAY_SECS: u32 = 300;

pub struct ShutdownSystem;

#[async_trait]
impl Tool for ShutdownSystem {
    fn name(&self) -> &str {
        "system.shutdown"
    }

    fn description(&self) -> &str {
        "Shut the computer down after a short countdown. Destructive — always confirm with the user first."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "delay_secs": {
                    "type": "integer",
                    "description": "Countdown in seconds before shutdown begins. Defaults to 10.",
                    "minimum": 0,
                    "maximum": MAX_DELAY_SECS,
                }
            },
            "additionalProperties": false
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Destructive
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let delay = parse_delay(&args).map_err(|e| ToolError::invalid_args(self.name(), e))?;
        run_shutdown(&["/s", "/t", &delay.to_string()])
            .map_err(|e| ToolError::execution(self.name(), e))?;
        Ok(ToolResult::new(format!(
            "Shutdown scheduled in {} seconds. Say cancel to abort.",
            delay
        )))
    }
}

pub(super) fn parse_delay(args: &Value) -> Result<u32, String> {
    let Some(raw) = args.get("delay_secs") else {
        return Ok(DEFAULT_DELAY_SECS);
    };
    let n = raw.as_u64().ok_or("delay_secs must be a non-negative integer")?;
    if n > MAX_DELAY_SECS as u64 {
        return Err(format!("delay_secs must be <= {}", MAX_DELAY_SECS));
    }
    Ok(n as u32)
}

pub(super) fn run_shutdown(args: &[&str]) -> Result<(), String> {
    let status = Command::new("shutdown.exe")
        .args(args)
        .status()
        .map_err(|e| format!("failed to invoke shutdown.exe: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "shutdown.exe exited with status {}",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".into())
        ))
    }
}
