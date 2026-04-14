//! Persistent memory for Ren.
//!
//! Two artefacts live under `%APPDATA%\Ren\memory\`:
//!
//! - `profile.md` — long-lived, human-editable markdown that captures who the
//!   user is and what Ren should keep in mind. Injected into the system
//!   prompt on every turn. Edits via the `memory.remember` / `memory.forget`
//!   tools always create a `profile.md.bak` rolling backup first.
//! - `conversations/YYYY-MM-DD.jsonl` — append-only per-day log of user
//!   transcripts and final assistant replies. The most recent entries are
//!   pulled into the prompt for cross-turn continuity. Old days are pruned
//!   on a configurable retention window.
//!
//! The clipboard preamble (Phase 8.2) is intentionally *not* archived — only
//! the raw user transcript ever lands here. This keeps potentially sensitive
//! pasted content from leaking into long-term storage.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::app_data_dir;
use crate::config::defaults::APP_DIR_NAME;

pub const PROFILE_FILENAME: &str = "profile.md";
pub const PROFILE_BACKUP_FILENAME: &str = "profile.md.bak";
pub const ARCHIVE_DIRNAME: &str = "conversations";
pub const NOTED_SECTION_HEADER: &str = "## Noted";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub timestamp: String,
    pub role: ArchiveRole,
    pub text: String,
}

/// Filesystem-backed store. Cheap to construct (just resolves paths); all
/// IO happens lazily on read/write.
pub struct MemoryStore {
    base: PathBuf,
}

impl MemoryStore {
    pub fn open() -> Result<Self> {
        let base = app_data_dir()?.join("memory");
        fs::create_dir_all(&base).with_context(|| {
            format!("Failed to create memory directory under {}", APP_DIR_NAME)
        })?;
        fs::create_dir_all(base.join(ARCHIVE_DIRNAME))
            .context("Failed to create conversations archive directory")?;
        Ok(Self { base })
    }

    pub fn profile_path(&self) -> PathBuf {
        self.base.join(PROFILE_FILENAME)
    }

    fn backup_path(&self) -> PathBuf {
        self.base.join(PROFILE_BACKUP_FILENAME)
    }

    fn archive_path_for_today(&self) -> PathBuf {
        let today = Local::now().format("%Y-%m-%d").to_string();
        self.base.join(ARCHIVE_DIRNAME).join(format!("{}.jsonl", today))
    }

    /// Reads `profile.md`. Returns an empty string when the file does not
    /// exist (first-time use is the common case).
    pub fn load_profile(&self) -> Result<String> {
        let path = self.profile_path();
        if !path.exists() {
            return Ok(String::new());
        }
        fs::read_to_string(&path)
            .with_context(|| format!("Failed to read profile at {}", path.display()))
    }

    /// Appends `fact` under the "## Noted" section with a timestamp prefix.
    /// Creates the section header if it does not exist yet. Always rolls a
    /// `.bak` first so a fat-fingered LLM cannot wipe the file.
    pub fn remember(&self, fact: &str) -> Result<()> {
        let trimmed = fact.trim();
        if trimmed.is_empty() {
            anyhow::bail!("Refusing to remember an empty fact");
        }

        let current = self.load_profile()?;
        let _ = self.write_backup(&current);

        let stamp = Local::now().format("%Y-%m-%d");
        let line = format!("- ({}) {}", stamp, trimmed);

        let new_contents = if current.contains(NOTED_SECTION_HEADER) {
            // Append to existing Noted section: insert the line right after
            // the header instead of at file end so newest items stay near
            // the title block.
            let mut updated = String::with_capacity(current.len() + line.len() + 2);
            let mut inserted = false;
            for raw_line in current.lines() {
                updated.push_str(raw_line);
                updated.push('\n');
                if !inserted && raw_line.trim_end() == NOTED_SECTION_HEADER {
                    updated.push_str(&line);
                    updated.push('\n');
                    inserted = true;
                }
            }
            updated
        } else {
            let mut updated = current.clone();
            if !updated.is_empty() && !updated.ends_with('\n') {
                updated.push('\n');
            }
            if !updated.is_empty() {
                updated.push('\n');
            }
            updated.push_str(NOTED_SECTION_HEADER);
            updated.push('\n');
            updated.push_str(&line);
            updated.push('\n');
            updated
        };

        fs::write(self.profile_path(), new_contents)
            .with_context(|| format!("Failed to write profile at {}", self.profile_path().display()))
    }

    /// Removes every line in the profile that contains `query` (case
    /// insensitive). Returns the number of removed lines so the caller can
    /// narrate "forgot N items". Always backs up first.
    pub fn forget_matching(&self, query: &str) -> Result<usize> {
        let query_lc = query.trim().to_lowercase();
        if query_lc.is_empty() {
            anyhow::bail!("Refusing to forget against an empty query");
        }

        let current = self.load_profile()?;
        if current.is_empty() {
            return Ok(0);
        }
        let _ = self.write_backup(&current);

        let mut kept = Vec::with_capacity(current.lines().count());
        let mut removed = 0usize;
        for line in current.lines() {
            if line.to_lowercase().contains(&query_lc) {
                removed += 1;
                continue;
            }
            kept.push(line);
        }

        if removed == 0 {
            return Ok(0);
        }
        let mut new_contents = kept.join("\n");
        if !new_contents.ends_with('\n') {
            new_contents.push('\n');
        }
        fs::write(self.profile_path(), new_contents)
            .with_context(|| format!("Failed to write profile at {}", self.profile_path().display()))?;
        Ok(removed)
    }

    fn write_backup(&self, contents: &str) -> Result<()> {
        if contents.is_empty() {
            return Ok(());
        }
        fs::write(self.backup_path(), contents)
            .with_context(|| format!("Failed to write backup at {}", self.backup_path().display()))
    }

    /// Appends one turn to today's archive file. Errors are logged but not
    /// surfaced — a transient archive failure should never break the live
    /// conversation.
    pub fn archive_turn(&self, role: ArchiveRole, text: &str) {
        let entry = ArchiveEntry {
            timestamp: Local::now().to_rfc3339(),
            role,
            text: text.to_string(),
        };
        let json = match serde_json::to_string(&entry) {
            Ok(j) => j,
            Err(e) => {
                warn!("Memory archive serialize failed: {}", e);
                return;
            }
        };
        let path = self.archive_path_for_today();
        if let Err(e) = append_line(&path, &json) {
            warn!("Memory archive write failed ({}): {}", path.display(), e);
        }
    }

    /// Returns the last `n` archive entries (most recent first). Walks
    /// today's file then yesterday's so a long conversation that crosses
    /// midnight still stays continuous.
    pub fn recent_entries(&self, n: usize) -> Vec<ArchiveEntry> {
        if n == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(n);
        let today = Local::now().date_naive();
        for offset in 0..=1 {
            let day = today - chrono::Duration::days(offset);
            let path = self
                .base
                .join(ARCHIVE_DIRNAME)
                .join(format!("{}.jsonl", day));
            if !path.exists() {
                continue;
            }
            let contents = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for line in contents.lines().rev() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<ArchiveEntry>(line) {
                    out.push(entry);
                    if out.len() >= n {
                        return out;
                    }
                }
            }
        }
        out
    }

    /// Deletes archive files older than `retention_days`. Best-effort: any
    /// individual delete failure is logged and skipped.
    pub fn prune_archive(&self, retention_days: u32) {
        if retention_days == 0 {
            return;
        }
        let cutoff = Local::now().date_naive() - chrono::Duration::days(retention_days as i64);
        let dir = self.base.join(ARCHIVE_DIRNAME);
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if let Ok(date) = chrono::NaiveDate::parse_from_str(&stem, "%Y-%m-%d") {
                if date < cutoff {
                    if let Err(e) = fs::remove_file(&path) {
                        warn!("Memory prune failed for {}: {}", path.display(), e);
                    }
                }
            }
        }
    }
}

fn append_line(path: &PathBuf, line: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", line)?;
    Ok(())
}

/// Markdown block injected into the system prompt. Empty when there is no
/// profile yet — the prompt builder simply skips the block in that case.
pub fn prompt_block(profile_md: &str, recent: &[ArchiveEntry]) -> String {
    let profile_trimmed = profile_md.trim();
    let mut buf = String::new();

    if !profile_trimmed.is_empty() {
        buf.push_str("Persistent memory of the user:\n");
        buf.push_str(profile_trimmed);
        buf.push('\n');
    }

    if !recent.is_empty() {
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str("Recent conversation excerpts (most recent first):\n");
        for entry in recent {
            let role = match entry.role {
                ArchiveRole::User => "user",
                ArchiveRole::Assistant => "assistant",
            };
            let snippet = entry.text.chars().take(240).collect::<String>();
            buf.push_str(&format!("- [{}] {}: {}\n", entry.timestamp, role, snippet));
        }
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn isolated() -> MemoryStore {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!("ren-mem-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(base.join(ARCHIVE_DIRNAME)).unwrap();
        MemoryStore { base }
    }

    #[test]
    fn remember_creates_noted_section() {
        let store = isolated();
        store.remember("user prefers metric").unwrap();
        let body = store.load_profile().unwrap();
        assert!(body.contains(NOTED_SECTION_HEADER));
        assert!(body.contains("user prefers metric"));
    }

    #[test]
    fn remember_writes_backup_after_first_entry() {
        let store = isolated();
        store.remember("first").unwrap();
        store.remember("second").unwrap();
        let backup = std::fs::read_to_string(store.backup_path()).unwrap();
        assert!(backup.contains("first"));
        assert!(!backup.contains("second"));
    }

    #[test]
    fn forget_removes_matching_lines() {
        let store = isolated();
        store.remember("loves coffee").unwrap();
        store.remember("hates tea").unwrap();
        let removed = store.forget_matching("coffee").unwrap();
        assert_eq!(removed, 1);
        let body = store.load_profile().unwrap();
        assert!(!body.contains("loves coffee"));
        assert!(body.contains("hates tea"));
    }

    #[test]
    fn forget_returns_zero_when_no_match() {
        let store = isolated();
        store.remember("anything").unwrap();
        let removed = store.forget_matching("nothing-like-this").unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn archive_round_trips() {
        let store = isolated();
        store.archive_turn(ArchiveRole::User, "hello");
        store.archive_turn(ArchiveRole::Assistant, "hi");
        let recent = store.recent_entries(5);
        assert_eq!(recent.len(), 2);
        // recent_entries returns most recent first
        assert!(matches!(recent[0].role, ArchiveRole::Assistant));
        assert_eq!(recent[0].text, "hi");
    }

    #[test]
    fn prompt_block_is_empty_when_nothing_stored() {
        let block = prompt_block("", &[]);
        assert!(block.is_empty());
    }
}
