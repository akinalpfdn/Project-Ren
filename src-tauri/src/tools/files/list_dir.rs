//! `files.list_dir` — enumerate entries inside one of the user's standard
//! folders (or a relative subpath). Output is intentionally compact so the
//! LLM can quote it back without burning context: name, kind, size, mtime.

use async_trait::async_trait;
use chrono::{DateTime, Local};
use serde_json::{json, Value};

use crate::tools::files::paths::{display_relative, resolve, ALLOWED_FOLDERS};
use crate::tools::{Tool, ToolError, ToolResult};

/// Defensive cap on how many entries we surface in a single response.
const MAX_ENTRIES: usize = 50;

pub struct ListDir;

#[async_trait]
impl Tool for ListDir {
    fn name(&self) -> &str {
        "files.list_dir"
    }

    fn description(&self) -> &str {
        "List files and subfolders inside one of the user's standard folders \
         (Downloads, Documents, Desktop, Pictures, Music, Videos), optionally \
         under a relative subpath. Returns the most recently modified entries \
         first, capped at a sensible limit."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "folder": {
                    "type": "string",
                    "enum": ALLOWED_FOLDERS,
                    "description": "Which standard folder to list under."
                },
                "subpath": {
                    "type": "string",
                    "description": "Optional path inside the folder, using forward or back slashes. Leave empty for the folder root."
                }
            },
            "required": ["folder"],
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
            .map(str::to_string);

        let target = resolve(&folder, subpath.as_deref())
            .map_err(|e| ToolError::invalid_args(self.name(), e.to_string()))?;

        if !target.is_dir() {
            return Err(ToolError::invalid_args(
                self.name(),
                format!("{} is not a directory", target.display()),
            ));
        }

        let mut entries = tokio::task::spawn_blocking(move || -> Result<Vec<Entry>, std::io::Error> {
            let mut out = Vec::new();
            for dir_entry in std::fs::read_dir(&target)? {
                let dir_entry = match dir_entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let metadata = match dir_entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                out.push(Entry {
                    name: dir_entry.file_name().to_string_lossy().to_string(),
                    is_dir: metadata.is_dir(),
                    size: if metadata.is_file() { metadata.len() } else { 0 },
                    modified: metadata.modified().ok().and_then(|t| {
                        DateTime::<Local>::from(t).format("%Y-%m-%d %H:%M").to_string().into()
                    }),
                });
            }
            // Newest first — most useful for "the latest file in Downloads" queries.
            out.sort_by(|a, b| b.modified.cmp(&a.modified));
            out.truncate(MAX_ENTRIES);
            Ok(out)
        })
        .await
        .map_err(|e| ToolError::execution(self.name(), format!("join error: {}", e)))?
        .map_err(|e| ToolError::execution(self.name(), format!("read_dir failed: {}", e)))?;

        let listing_label = display_relative(&folder.to_ascii_lowercase(), &resolve(&folder, subpath.as_deref())
            .map_err(|e| ToolError::execution(self.name(), e.to_string()))?);

        let summary = if entries.is_empty() {
            format!("{} is empty.", listing_label)
        } else {
            format!(
                "{} contains {} entries (showing most recent first).",
                listing_label,
                entries.len()
            )
        };

        let detail_lines: Vec<String> = entries
            .drain(..)
            .map(|e| {
                let kind = if e.is_dir { "dir" } else { "file" };
                let size = if e.is_dir {
                    "—".to_string()
                } else {
                    human_size(e.size)
                };
                let when = e.modified.unwrap_or_else(|| "?".to_string());
                format!("{:<5} {:<20} {:<12} {}", kind, e.name, size, when)
            })
            .collect();

        Ok(ToolResult::with_detail(summary, detail_lines.join("\n")))
    }
}

#[derive(Debug)]
struct Entry {
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<String>,
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(2_048), "2.0 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
