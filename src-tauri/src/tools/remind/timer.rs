//! In-memory timers — precise, ephemeral, per-entry `tokio::time::sleep`.
//!
//! The registry owns the map of active timers; tool calls from the LLM
//! mutate it through the `Arc<Mutex<...>>` wrapper. When a timer's sleep
//! future resolves it posts a `Firing` and removes itself from the map.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tauri::async_runtime::JoinHandle;
use tokio::time::Instant;
use tracing::warn;

use crate::tools::remind::{FireSender, Firing};
use crate::tools::{Tool, ToolError, ToolResult};

/// Upper bound on how long a timer can run. 24h is more than any realistic
/// use and keeps the LLM from smuggling arbitrary dates past `time.until`.
const MAX_DURATION_SECS: u64 = 24 * 3600;

pub struct TimerRegistry {
    next_id: AtomicU64,
    entries: Mutex<HashMap<u64, TimerEntry>>,
    fire_tx: FireSender,
}

pub type SharedTimerRegistry = Arc<TimerRegistry>;

struct TimerEntry {
    label: String,
    fires_at: Instant,
    handle: JoinHandle<()>,
}

impl TimerRegistry {
    pub fn new(fire_tx: FireSender) -> SharedTimerRegistry {
        Arc::new(Self {
            next_id: AtomicU64::new(1),
            entries: Mutex::new(HashMap::new()),
            fire_tx,
        })
    }

    pub fn start(&self, duration_secs: u64, label: String) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let fires_at = Instant::now() + Duration::from_secs(duration_secs);
        let fire_tx = self.fire_tx.clone();
        let label_for_task = label.clone();

        let handle = tauri::async_runtime::spawn(async move {
            tokio::time::sleep_until(fires_at).await;
            if let Err(e) = fire_tx
                .send(Firing {
                    kind: "timer",
                    label: label_for_task,
                })
                .await
            {
                warn!("Timer fire channel closed: {}", e);
            }
        });

        self.entries.lock().unwrap().insert(
            id,
            TimerEntry {
                label,
                fires_at,
                handle,
            },
        );
        id
    }

    pub fn cancel(&self, id: u64) -> bool {
        if let Some(entry) = self.entries.lock().unwrap().remove(&id) {
            entry.handle.abort();
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> Vec<(u64, String, u64)> {
        let now = Instant::now();
        self.entries
            .lock()
            .unwrap()
            .iter()
            .map(|(id, entry)| {
                let remaining = entry
                    .fires_at
                    .checked_duration_since(now)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (*id, entry.label.clone(), remaining)
            })
            .collect()
    }

    /// Called by the fire consumer once a timer has actually fired, so we
    /// can drop its bookkeeping entry. Safe no-op if already removed.
    pub fn forget_by_label(&self, label: &str) {
        let mut guard = self.entries.lock().unwrap();
        let target = guard
            .iter()
            .find(|(_, e)| e.label == label && e.fires_at <= Instant::now())
            .map(|(id, _)| *id);
        if let Some(id) = target {
            guard.remove(&id);
        }
    }
}

pub struct TimerStart {
    pub registry: SharedTimerRegistry,
}

pub struct TimerList {
    pub registry: SharedTimerRegistry,
}

pub struct TimerCancel {
    pub registry: SharedTimerRegistry,
}

#[async_trait]
impl Tool for TimerStart {
    fn name(&self) -> &str {
        "timer.start"
    }

    fn description(&self) -> &str {
        "Start a countdown timer. When it fires Ren speaks the label aloud. \
         Use this for any 'remind me in N minutes' phrasing where N is a \
         relative duration rather than a clock time."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "duration_seconds": {
                    "type": "integer",
                    "minimum": 5,
                    "maximum": MAX_DURATION_SECS,
                    "description": "How long to wait before firing, in seconds."
                },
                "label": {
                    "type": "string",
                    "description": "Short label Ren narrates when the timer goes off (e.g. 'tea', 'call mum')."
                }
            },
            "required": ["duration_seconds", "label"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let duration = args
            .get("duration_seconds")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing 'duration_seconds'"))?;
        if !(5..=MAX_DURATION_SECS).contains(&duration) {
            return Err(ToolError::invalid_args(
                self.name(),
                format!("duration must be between 5 and {} seconds", MAX_DURATION_SECS),
            ));
        }
        let label = args
            .get("label")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing or empty 'label'"))?
            .to_string();

        let id = self.registry.start(duration, label.clone());
        Ok(ToolResult::new(format!(
            "Timer #{} set for {} — firing in {}.",
            id,
            label,
            human_duration(duration)
        )))
    }
}

#[async_trait]
impl Tool for TimerList {
    fn name(&self) -> &str {
        "timer.list"
    }

    fn description(&self) -> &str {
        "List active timers with their labels and remaining time."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        let entries = self.registry.list();
        if entries.is_empty() {
            return Ok(ToolResult::new("No active timers.".to_string()));
        }
        let detail_lines: Vec<String> = entries
            .iter()
            .map(|(id, label, remaining)| {
                format!("#{:<3} {:<20} {}", id, label, human_duration(*remaining))
            })
            .collect();
        Ok(ToolResult::with_detail(
            format!("{} active timer(s).", entries.len()),
            detail_lines.join("\n"),
        ))
    }
}

#[async_trait]
impl Tool for TimerCancel {
    fn name(&self) -> &str {
        "timer.cancel"
    }

    fn description(&self) -> &str {
        "Cancel an active timer by its id (from timer.list)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Timer id as reported by timer.list or timer.start."
                }
            },
            "required": ["id"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing 'id'"))?;
        let removed = self.registry.cancel(id);
        Ok(ToolResult::new(if removed {
            format!("Timer #{} cancelled.", id)
        } else {
            format!("Timer #{} was not active.", id)
        }))
    }
}

fn human_duration(seconds: u64) -> String {
    let h = seconds / 3_600;
    let m = (seconds % 3_600) / 60;
    let s = seconds % 60;
    let mut parts = Vec::new();
    if h > 0 {
        parts.push(format!("{}h", h));
    }
    if m > 0 || h > 0 {
        parts.push(format!("{}m", m));
    }
    if s > 0 || parts.is_empty() {
        parts.push(format!("{}s", s));
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_duration_covers_common_shapes() {
        assert_eq!(human_duration(45), "45s");
        assert_eq!(human_duration(120), "2m");
        assert_eq!(human_duration(3_600), "1h 0m");
        assert_eq!(human_duration(3_661), "1h 1m 1s");
    }
}
