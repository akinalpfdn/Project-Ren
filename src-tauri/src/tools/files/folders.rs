//! `files.open_folder` — opens one of the user's standard folders in
//! Windows Explorer. The set is intentionally small (Downloads, Documents,
//! Desktop, Pictures, Music, Videos) so the LLM can map a spoken phrase to
//! one of a known enum rather than passing an arbitrary path — which would
//! be a security footgun.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolError, ToolResult};

/// Every folder the tool is allowed to open. Adding a new variant here is
/// the only supported way to expose another location.
const ALLOWED_FOLDERS: &[&str] = &[
    "downloads",
    "documents",
    "desktop",
    "pictures",
    "music",
    "videos",
];

pub struct OpenFolder;

#[async_trait]
impl Tool for OpenFolder {
    fn name(&self) -> &str {
        "files.open_folder"
    }

    fn description(&self) -> &str {
        "Open one of the user's standard folders (Downloads, Documents, Desktop, Pictures, \
         Music, Videos) in File Explorer."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "folder": {
                    "type": "string",
                    "enum": ALLOWED_FOLDERS,
                    "description": "Which standard folder to open."
                }
            },
            "required": ["folder"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Accept every reasonable key the model tends to invent —
        // `folder` is canonical but Qwen flips to `path`, `folder_path`,
        // `dir`, `directory` between turns.
        let raw = args
            .get("folder")
            .or_else(|| args.get("path"))
            .or_else(|| args.get("folder_path"))
            .or_else(|| args.get("dir"))
            .or_else(|| args.get("directory"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing 'folder'"))?;
        let key = raw.to_ascii_lowercase();

        if !ALLOWED_FOLDERS.contains(&key.as_str()) {
            return Err(ToolError::invalid_args(
                self.name(),
                format!("unknown folder '{}'", raw),
            ));
        }

        let path = resolve_folder(&key)
            .ok_or_else(|| ToolError::execution(self.name(), format!("could not resolve '{}'", key)))?;

        if !path.exists() {
            return Err(ToolError::execution(
                self.name(),
                format!("folder not found at {}", path.display()),
            ));
        }

        open_in_explorer(&path).map_err(|e| ToolError::execution(self.name(), e))?;
        Ok(ToolResult::new(format!("Opening {}.", display_name(&key))))
    }
}

fn display_name(key: &str) -> &'static str {
    match key {
        "downloads" => "Downloads",
        "documents" => "Documents",
        "desktop" => "Desktop",
        "pictures" => "Pictures",
        "music" => "Music",
        "videos" => "Videos",
        _ => "folder",
    }
}

fn resolve_folder(key: &str) -> Option<PathBuf> {
    // Windows 10/11 with OneDrive redirects Desktop / Documents / Pictures
    // away from `%USERPROFILE%`. `UserDirs` goes through
    // SHGetKnownFolderPath and returns the real current location.
    let user = directories::UserDirs::new()?;
    let path: Option<&Path> = match key {
        "downloads" => user.download_dir(),
        "documents" => user.document_dir(),
        "desktop" => user.desktop_dir(),
        "pictures" => user.picture_dir(),
        "music" => user.audio_dir(),
        "videos" => user.video_dir(),
        _ => None,
    };
    path.map(PathBuf::from)
}

#[cfg(windows)]
fn open_in_explorer(path: &Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map_err(|e| format!("failed to open {}: {}", path.display(), e))?;
    Ok(())
}

#[cfg(not(windows))]
fn open_in_explorer(_path: &Path) -> Result<(), String> {
    Err("opening folders is only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn rejects_unknown_folder() {
        let tool = OpenFolder;
        let err = tool.execute(json!({ "folder": "secrets" })).await.unwrap_err();
        matches!(err, ToolError::InvalidArgs { .. });
    }

    #[tokio::test]
    async fn rejects_missing_folder() {
        let tool = OpenFolder;
        let err = tool.execute(json!({})).await.unwrap_err();
        matches!(err, ToolError::InvalidArgs { .. });
    }
}
