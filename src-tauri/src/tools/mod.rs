//! Tool system — lets Ren take real actions on the user's machine.
//!
//! Every capability (open an app, launch a Steam game, set system volume,
//! search the web) implements `Tool`. The `ToolRegistry` owns the set of
//! available tools, exposes their JSON Schemas for LLM function calling,
//! and dispatches calls to the right executor.

pub mod apps;
pub mod files;
pub mod media;
pub mod memory;
pub mod registry;
pub mod remind;
pub mod steam;
pub mod system;
pub mod time;
pub mod weather;
pub mod web;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use registry::ToolRegistry;

/// Outcome of a successful tool call.
///
/// `summary` is the short human-readable line the LLM can narrate back to
/// the user ("Chrome launched", "Volume set to 30 %"). `detail` carries
/// extra structured data that the LLM may quote verbatim — for example a
/// weather reading or a web search snippet list.
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ToolResult {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            detail: None,
        }
    }

    pub fn with_detail(summary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            detail: Some(detail.into()),
        }
    }
}

/// Typed error surface for tool execution failures.
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool '{0}' is not registered")]
    NotFound(String),

    #[error("invalid arguments for tool '{tool}': {reason}")]
    InvalidArgs { tool: String, reason: String },

    #[error("tool '{tool}' failed: {reason}")]
    Execution { tool: String, reason: String },

    #[error("tool '{tool}' is not supported on this platform")]
    Unsupported { tool: String },

    #[error("tool '{tool}' requires user configuration ({missing})")]
    MissingConfig { tool: String, missing: String },
}

impl ToolError {
    pub fn invalid_args(tool: &str, reason: impl Into<String>) -> Self {
        Self::InvalidArgs {
            tool: tool.into(),
            reason: reason.into(),
        }
    }

    pub fn execution(tool: &str, reason: impl Into<String>) -> Self {
        Self::Execution {
            tool: tool.into(),
            reason: reason.into(),
        }
    }
}

/// Tools that mutate system state (shutdown, restart) must opt into
/// confirmation so the LLM asks the user before the action fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolSafety {
    /// Safe to call without confirmation (open app, read weather).
    Safe,
    /// Requires spoken confirmation before execution (shutdown, restart).
    Destructive,
}

/// Strategy trait every capability implements.
///
/// `execute` is async so tools can do HTTP calls, parse caches from disk,
/// or invoke blocking Windows APIs on a dedicated thread.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique identifier the LLM uses in tool calls (e.g. `"system.volume"`).
    fn name(&self) -> &str;

    /// Short description included in the tool schema the LLM sees.
    fn description(&self) -> &str;

    /// JSON Schema describing the arguments object — Ollama-compatible shape.
    fn parameters(&self) -> serde_json::Value;

    /// Default is `Safe`. Override for destructive tools.
    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    /// Run the tool. Implementations should not log sensitive data.
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError>;
}

/// Tauri event payloads for the frontend tool-card UI.
pub mod events {
    use serde::Serialize;

    #[derive(Debug, Clone, Serialize)]
    pub struct ToolExecuting {
        pub tool: String,
        pub description: String,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct ToolResultEvent {
        pub tool: String,
        pub success: bool,
        pub summary: String,
    }

    pub const EVENT_TOOL_EXECUTING: &str = "ren://tool-executing";
    pub const EVENT_TOOL_RESULT: &str = "ren://tool-result";
}
