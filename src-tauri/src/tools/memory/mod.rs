//! `memory.remember` / `memory.forget` — let the LLM curate the persistent
//! profile that ships back into every system prompt. Both tools route
//! through `crate::memory::MemoryStore` which handles the on-disk format,
//! rolling backup, and case-insensitive matching.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::memory::MemoryStore;
use crate::tools::{Tool, ToolError, ToolResult};

pub struct Remember;
pub struct Forget;

#[async_trait]
impl Tool for Remember {
    fn name(&self) -> &str {
        "memory.remember"
    }

    fn description(&self) -> &str {
        "Persist a short fact about the user under a 'Noted' section in the \
         profile. Use this when the user explicitly asks to be remembered \
         ('remember my name is Akın', 'I prefer Celsius', 'I live in Istanbul'). \
         Do not use it to log opinions, transient state, or anything sensitive."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "fact": {
                    "type": "string",
                    "description": "A single short fact to remember, in plain English."
                }
            },
            "required": ["fact"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let fact = args
            .get("fact")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing or empty 'fact'"))?
            .to_string();

        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let store = MemoryStore::open().map_err(|e| e.to_string())?;
            store.remember(&fact).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| ToolError::execution(self.name(), format!("join error: {}", e)))?
        .map_err(|e| ToolError::execution(self.name(), e))?;

        Ok(ToolResult::new("Noted, sir."))
    }
}

#[async_trait]
impl Tool for Forget {
    fn name(&self) -> &str {
        "memory.forget"
    }

    fn description(&self) -> &str {
        "Remove every line from the persistent profile that matches the given \
         substring (case insensitive). Use this when the user says 'forget X', \
         'I no longer …', or wants to clear something specific."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Substring to match against profile lines. The shortest unique fragment works best."
                }
            },
            "required": ["query"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing or empty 'query'"))?
            .to_string();

        let removed = tokio::task::spawn_blocking(move || -> Result<usize, String> {
            let store = MemoryStore::open().map_err(|e| e.to_string())?;
            store.forget_matching(&query).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| ToolError::execution(self.name(), format!("join error: {}", e)))?
        .map_err(|e| ToolError::execution(self.name(), e))?;

        let summary = match removed {
            0 => "Nothing in the profile matched that — already forgotten.".to_string(),
            1 => "Removed one entry from the profile.".to_string(),
            n => format!("Removed {} entries from the profile.", n),
        };
        Ok(ToolResult::new(summary))
    }
}
