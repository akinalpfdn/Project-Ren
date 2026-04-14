//! Media transport controls — plays nicely with whatever media app happens
//! to have the current Windows session (Spotify, browser YouTube, VLC, ...).
//!
//! Backed by the Windows Runtime `GlobalSystemMediaTransportControlsSession`
//! API. Each tool calls into `session.rs` which takes care of grabbing the
//! current session on a blocking thread and hands back a typed result.

mod session;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolError, ToolResult};

use session::{next_track, pause, play, previous_track, try_current_track};

/// Resumes playback of whatever app currently owns the system media session.
pub struct MediaPlay;

/// Pauses playback of whatever app currently owns the system media session.
pub struct MediaPause;

/// Skips to the next track in the current media app.
pub struct MediaNext;

/// Skips back to the previous track in the current media app.
pub struct MediaPrevious;

/// Reads the title / artist of the currently playing track.
pub struct MediaCurrentTrack;

fn empty_object_schema() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

#[async_trait]
impl Tool for MediaPlay {
    fn name(&self) -> &str {
        "media.play"
    }

    fn description(&self) -> &str {
        "Resume whatever is currently paused on the system media session \
         (Spotify, YouTube in a browser tab, VLC, etc)."
    }

    fn parameters(&self) -> Value {
        empty_object_schema()
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        run_media(self.name(), play).await?;
        Ok(ToolResult::new("Playback resumed."))
    }
}

#[async_trait]
impl Tool for MediaPause {
    fn name(&self) -> &str {
        "media.pause"
    }

    fn description(&self) -> &str {
        "Pause whatever is currently playing on the system media session."
    }

    fn parameters(&self) -> Value {
        empty_object_schema()
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        run_media(self.name(), pause).await?;
        Ok(ToolResult::new("Playback paused."))
    }
}

#[async_trait]
impl Tool for MediaNext {
    fn name(&self) -> &str {
        "media.next"
    }

    fn description(&self) -> &str {
        "Skip to the next track in the current media app."
    }

    fn parameters(&self) -> Value {
        empty_object_schema()
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        run_media(self.name(), next_track).await?;
        Ok(ToolResult::new("Skipped to the next track."))
    }
}

#[async_trait]
impl Tool for MediaPrevious {
    fn name(&self) -> &str {
        "media.previous"
    }

    fn description(&self) -> &str {
        "Skip back to the previous track in the current media app."
    }

    fn parameters(&self) -> Value {
        empty_object_schema()
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        run_media(self.name(), previous_track).await?;
        Ok(ToolResult::new("Back to the previous track."))
    }
}

#[async_trait]
impl Tool for MediaCurrentTrack {
    fn name(&self) -> &str {
        "media.current_track"
    }

    fn description(&self) -> &str {
        "Return the title and artist of whatever is currently playing, or \
         say that nothing is playing if no session is active."
    }

    fn parameters(&self) -> Value {
        empty_object_schema()
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        let summary = tokio::task::spawn_blocking(try_current_track)
            .await
            .map_err(|e| ToolError::execution(self.name(), format!("join error: {}", e)))?
            .map_err(|e| ToolError::execution(self.name(), e))?;
        Ok(ToolResult::new(summary))
    }
}

/// Runs a synchronous WinRT media operation on a blocking thread so we never
/// stall the tokio runtime while Windows walks its session list.
async fn run_media<F>(tool: &str, op: F) -> Result<(), ToolError>
where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    tokio::task::spawn_blocking(op)
        .await
        .map_err(|e| ToolError::execution(tool, format!("join error: {}", e)))?
        .map_err(|e| ToolError::execution(tool, e))
}
