//! `files.read_text` — reads a UTF-8 text file from one of the user's
//! standard folders and returns the contents (clamped to a sensible byte
//! budget) so the LLM can summarise / quote / translate it.
//!
//! Binary files are rejected with a clear error rather than dumping invalid
//! UTF-8 into the prompt. PDF / DOCX parsing is intentionally out of scope
//! for this iteration.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::files::paths::{display_relative, resolve, ALLOWED_FOLDERS};
use crate::tools::{Tool, ToolError, ToolResult};

/// Hard cap on bytes read into the prompt. 32 KiB ≈ 8 k tokens, comfortably
/// within Qwen's context after the system prompt and conversation history.
const MAX_BYTES: u64 = 32 * 1024;

pub struct ReadText;

#[async_trait]
impl Tool for ReadText {
    fn name(&self) -> &str {
        "files.read_text"
    }

    fn description(&self) -> &str {
        "Read a UTF-8 text file under one of the user's standard folders and \
         return its contents. Use this to summarise notes, quote logs, or \
         translate documents the user already has on disk. Caps the response \
         around 32 KiB; binary files are rejected."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "folder": {
                    "type": "string",
                    "enum": ALLOWED_FOLDERS,
                    "description": "Which standard folder the file lives under."
                },
                "subpath": {
                    "type": "string",
                    "description": "Path to the file inside that folder, e.g. 'notes/meeting.md'."
                }
            },
            "required": ["folder", "subpath"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let folder = args
            .get("folder")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing 'folder'"))?
            .to_string();
        let subpath = args
            .get("subpath")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing or empty 'subpath'"))?
            .to_string();

        let target = resolve(&folder, Some(&subpath))
            .map_err(|e| ToolError::invalid_args(self.name(), e.to_string()))?;

        if !target.is_file() {
            return Err(ToolError::invalid_args(
                self.name(),
                format!("{} is not a regular file", target.display()),
            ));
        }

        let target_for_read = target.clone();
        let bytes_read = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, std::io::Error> {
            use std::io::Read;
            let mut file = std::fs::File::open(&target_for_read)?;
            let mut buf = Vec::with_capacity(MAX_BYTES as usize);
            file.take(MAX_BYTES).read_to_end(&mut buf)?;
            Ok(buf)
        })
        .await
        .map_err(|e| ToolError::execution(self.name(), format!("join error: {}", e)))?
        .map_err(|e| ToolError::execution(self.name(), format!("open/read failed: {}", e)))?;

        let text = String::from_utf8(bytes_read).map_err(|_| {
            ToolError::execution(
                self.name(),
                "file is not valid UTF-8 (binary files are not supported yet)".to_string(),
            )
        })?;

        let label = display_relative(&folder.to_ascii_lowercase(), &target);
        let metadata = std::fs::metadata(&target)
            .map_err(|e| ToolError::execution(self.name(), format!("stat failed: {}", e)))?;
        let truncated = metadata.len() > MAX_BYTES;
        let summary = if truncated {
            format!(
                "Read first {} KiB of {} (file is {} bytes total — truncated).",
                MAX_BYTES / 1024,
                label,
                metadata.len()
            )
        } else {
            format!("Read {} ({} bytes).", label, metadata.len())
        };

        Ok(ToolResult::with_detail(summary, text))
    }
}
