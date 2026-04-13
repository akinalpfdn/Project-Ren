//! `system.restart` — schedule an OS restart via `shutdown.exe /r`.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::shutdown::{parse_delay, run_shutdown};
use crate::tools::{Tool, ToolError, ToolResult, ToolSafety};

const MAX_DELAY_SECS: u32 = 300;

pub struct RestartSystem;

#[async_trait]
impl Tool for RestartSystem {
    fn name(&self) -> &str {
        "system.restart"
    }

    fn description(&self) -> &str {
        "Restart the computer after a short countdown. Destructive — always confirm with the user first."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "delay_secs": {
                    "type": "integer",
                    "description": "Countdown in seconds before restart begins. Defaults to 10.",
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
        run_shutdown(&["/r", "/t", &delay.to_string()])
            .map_err(|e| ToolError::execution(self.name(), e))?;
        Ok(ToolResult::new(format!(
            "Restart scheduled in {} seconds. Say cancel to abort.",
            delay
        )))
    }
}
