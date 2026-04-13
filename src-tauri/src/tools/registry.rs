//! Central registry that owns every available tool.
//!
//! The registry is built once at startup in `lib.rs`. The LLM layer queries
//! `ollama_tools()` for the JSON payload Ollama expects, and routes
//! incoming tool calls through `dispatch()`.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use super::{Tool, ToolError, ToolResult, ToolSafety};

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Later registrations with the same name replace earlier ones.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Ordered list of `(name, safety)` pairs — used by the prompt builder
    /// to describe which tools require confirmation.
    pub fn safety_map(&self) -> Vec<(String, ToolSafety)> {
        let mut out: Vec<(String, ToolSafety)> = self
            .tools
            .values()
            .map(|t| (t.name().to_string(), t.safety()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Returns the tools payload in Ollama's chat API shape:
    /// `[{ "type": "function", "function": { name, description, parameters } }]`.
    pub fn ollama_tools(&self) -> Value {
        let mut entries: Vec<Value> = self
            .tools
            .values()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name(),
                        "description": t.description(),
                        "parameters": t.parameters(),
                    }
                })
            })
            .collect();
        entries.sort_by(|a, b| {
            a["function"]["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["function"]["name"].as_str().unwrap_or(""))
        });
        Value::Array(entries)
    }

    /// Route an LLM tool call to the right executor.
    pub async fn dispatch(&self, name: &str, args: Value) -> Result<ToolResult, ToolError> {
        let tool = self
            .get(name)
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;
        tool.execute(args).await
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
