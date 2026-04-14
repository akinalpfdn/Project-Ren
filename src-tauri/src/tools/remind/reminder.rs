//! Persistent wall-clock reminders.
//!
//! Backed by `%APPDATA%\Ren\memory\reminders.json` (sharing the memory
//! folder so "anything Ren remembers about me" lives under one directory).
//! A once-per-minute poll loop compares each pending entry's `fires_at`
//! against `Local::now()`; anything past its deadline is marked fired and
//! pushed onto the shared `FireSender`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use crate::config::app_data_dir;
use crate::tools::remind::{FireSender, Firing};
use crate::tools::{Tool, ToolError, ToolResult};

const POLL_INTERVAL_SECS: u64 = 60;

pub type SharedReminderStore = Arc<ReminderStore>;

pub struct ReminderStore {
    path: PathBuf,
    entries: Mutex<HashMap<u64, ReminderEntry>>,
    next_id: Mutex<u64>,
    fire_tx: FireSender,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReminderEntry {
    id: u64,
    text: String,
    fires_at: String, // RFC3339 for stable on-disk format
    fired: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PersistedState {
    next_id: u64,
    reminders: Vec<ReminderEntry>,
}

impl ReminderStore {
    pub fn open(fire_tx: FireSender) -> Result<SharedReminderStore> {
        let memory_dir = app_data_dir()?.join("memory");
        fs::create_dir_all(&memory_dir).context("Failed to create memory directory")?;
        let path = memory_dir.join("reminders.json");

        let state = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read reminders at {}", path.display()))?;
            if raw.trim().is_empty() {
                PersistedState::default()
            } else {
                serde_json::from_str::<PersistedState>(&raw)
                    .context("Reminder file is corrupt — refusing to load")?
            }
        } else {
            PersistedState::default()
        };

        let mut entries = HashMap::with_capacity(state.reminders.len());
        let mut max_id = state.next_id;
        for entry in state.reminders {
            max_id = max_id.max(entry.id + 1);
            entries.insert(entry.id, entry);
        }

        Ok(Arc::new(Self {
            path,
            entries: Mutex::new(entries),
            next_id: Mutex::new(max_id.max(1)),
            fire_tx,
        }))
    }

    pub fn set(&self, fires_at: DateTime<Local>, text: String) -> Result<u64> {
        let mut next_id = self.next_id.lock().unwrap();
        let id = *next_id;
        *next_id += 1;
        drop(next_id);

        let entry = ReminderEntry {
            id,
            text,
            fires_at: fires_at.to_rfc3339(),
            fired: false,
        };
        self.entries.lock().unwrap().insert(id, entry);
        self.persist()?;
        Ok(id)
    }

    pub fn cancel(&self, id: u64) -> Result<bool> {
        let removed = self.entries.lock().unwrap().remove(&id).is_some();
        if removed {
            self.persist()?;
        }
        Ok(removed)
    }

    /// Non-fired entries only, sorted by ascending `fires_at`.
    pub fn list(&self) -> Vec<(u64, String, String)> {
        let mut out: Vec<_> = self
            .entries
            .lock()
            .unwrap()
            .values()
            .filter(|e| !e.fired)
            .map(|e| (e.id, e.text.clone(), e.fires_at.clone()))
            .collect();
        out.sort_by(|a, b| a.2.cmp(&b.2));
        out
    }

    /// Walks every entry and fires anything past its deadline. Returns the
    /// number of freshly fired entries so the caller can log.
    pub async fn tick(&self) -> usize {
        let now = Local::now();
        let due: Vec<(u64, String)> = {
            let mut guard = self.entries.lock().unwrap();
            let mut due = Vec::new();
            for entry in guard.values_mut() {
                if entry.fired {
                    continue;
                }
                if let Ok(fires_at) = DateTime::parse_from_rfc3339(&entry.fires_at) {
                    if fires_at.with_timezone(&Local) <= now {
                        entry.fired = true;
                        due.push((entry.id, entry.text.clone()));
                    }
                }
            }
            due
        };

        if due.is_empty() {
            return 0;
        }
        if let Err(e) = self.persist() {
            warn!("Reminder persist after tick failed: {}", e);
        }
        let fired = due.len();
        for (_, text) in due {
            if let Err(e) = self
                .fire_tx
                .send(Firing {
                    kind: "reminder",
                    label: text,
                })
                .await
            {
                warn!("Reminder fire channel closed: {}", e);
            }
        }
        fired
    }

    fn persist(&self) -> Result<()> {
        let snapshot = PersistedState {
            next_id: *self.next_id.lock().unwrap(),
            reminders: self
                .entries
                .lock()
                .unwrap()
                .values()
                .cloned()
                .collect(),
        };
        let json = serde_json::to_string_pretty(&snapshot)
            .context("Failed to serialize reminders")?;
        fs::write(&self.path, json)
            .with_context(|| format!("Failed to write reminders at {}", self.path.display()))
    }
}

/// Spawns the polling loop. The task exits when the fire channel is closed.
pub fn spawn_poll_loop(store: SharedReminderStore) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(POLL_INTERVAL_SECS));
        // Skip the "tick immediately on start" bias — we only care about
        // the steady cadence so freshly-set reminders don't race.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let fired = store.tick().await;
            if fired > 0 {
                tracing::info!("Fired {} reminder(s) this tick", fired);
            }
        }
    });
}

// ─── Tools ────────────────────────────────────────────────────────────────────

pub struct ReminderSet {
    pub store: SharedReminderStore,
}

pub struct ReminderList {
    pub store: SharedReminderStore,
}

pub struct ReminderCancel {
    pub store: SharedReminderStore,
}

#[async_trait]
impl Tool for ReminderSet {
    fn name(&self) -> &str {
        "remind.set"
    }

    fn description(&self) -> &str {
        "Schedule a reminder at a specific wall-clock moment. Accepts an \
         ISO-8601 timestamp (e.g. '2026-04-20T18:00:00+03:00'). Use this for \
         absolute times — for relative durations use timer.start instead."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "at_iso8601": {
                    "type": "string",
                    "description": "Target time in ISO-8601 format, including timezone offset."
                },
                "text": {
                    "type": "string",
                    "description": "What Ren should say when the reminder fires."
                }
            },
            "required": ["at_iso8601", "text"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let at = args
            .get("at_iso8601")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing 'at_iso8601'"))?;
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing or empty 'text'"))?
            .to_string();

        let fires_at = DateTime::parse_from_rfc3339(at)
            .map_err(|e| {
                ToolError::invalid_args(self.name(), format!("could not parse '{}': {}", at, e))
            })?
            .with_timezone(&Local);

        if fires_at <= Local::now() {
            return Err(ToolError::invalid_args(
                self.name(),
                "target time is already in the past",
            ));
        }

        let id = self
            .store
            .set(fires_at, text.clone())
            .map_err(|e| ToolError::execution(self.name(), e.to_string()))?;
        Ok(ToolResult::new(format!(
            "Reminder #{} set for {} — '{}'.",
            id,
            fires_at.format("%A %H:%M"),
            text
        )))
    }
}

#[async_trait]
impl Tool for ReminderList {
    fn name(&self) -> &str {
        "remind.list"
    }

    fn description(&self) -> &str {
        "List pending reminders in chronological order."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        let entries = self.store.list();
        if entries.is_empty() {
            return Ok(ToolResult::new("No pending reminders.".to_string()));
        }
        let detail_lines: Vec<String> = entries
            .iter()
            .map(|(id, text, fires_at)| format!("#{:<3} {} — {}", id, fires_at, text))
            .collect();
        Ok(ToolResult::with_detail(
            format!("{} pending reminder(s).", entries.len()),
            detail_lines.join("\n"),
        ))
    }
}

#[async_trait]
impl Tool for ReminderCancel {
    fn name(&self) -> &str {
        "remind.cancel"
    }

    fn description(&self) -> &str {
        "Cancel a pending reminder by its id (from remind.list)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Reminder id as reported by remind.list or remind.set."
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
        let removed = self
            .store
            .cancel(id)
            .map_err(|e| ToolError::execution(self.name(), e.to_string()))?;
        Ok(ToolResult::new(if removed {
            format!("Reminder #{} cancelled.", id)
        } else {
            format!("Reminder #{} was not pending.", id)
        }))
    }
}
