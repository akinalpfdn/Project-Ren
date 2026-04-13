//! `apps.launch` — fuzzy-match a user query against the installed Start
//! Menu entries and launch the best hit.
//!
//! The entry list is scanned lazily (on first use) and cached under
//! `%APPDATA%\Ren\cache\apps.json` with the discovery timestamp.
//! `refresh=true` in the tool args forces a rescan.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use strsim::jaro_winkler;
use tracing::{debug, warn};

use crate::config;
use crate::tools::{Tool, ToolError, ToolResult};

/// Jaro-Winkler score below which the match is considered unreliable.
const MATCH_SCORE_THRESHOLD: f64 = 0.82;

/// Common aliases users speak → canonical Start Menu entry name.
/// Checked before fuzzy matching and always wins if a key is hit.
const ALIASES: &[(&str, &str)] = &[
    ("chrome", "Google Chrome"),
    ("vscode", "Visual Studio Code"),
    ("vs code", "Visual Studio Code"),
    ("code", "Visual Studio Code"),
    ("edge", "Microsoft Edge"),
    ("firefox", "Mozilla Firefox"),
    ("photoshop", "Adobe Photoshop"),
    ("spotify", "Spotify"),
    ("discord", "Discord"),
    ("slack", "Slack"),
    ("notepad", "Notepad"),
    ("calc", "Calculator"),
    ("calculator", "Calculator"),
    ("file explorer", "File Explorer"),
    ("explorer", "File Explorer"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppEntry {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppCache {
    scanned_at_unix: u64,
    entries: Vec<AppEntry>,
}

pub struct AppLauncher {
    cache: RwLock<Option<Vec<AppEntry>>>,
}

impl AppLauncher {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(None),
        }
    }

    fn cache_path() -> Result<PathBuf, String> {
        config::cache_dir()
            .map(|p| p.join("apps.json"))
            .map_err(|e| e.to_string())
    }

    fn ensure_cache(&self, refresh: bool) -> Result<Vec<AppEntry>, String> {
        if !refresh {
            if let Some(entries) = self.cache.read().ok().and_then(|g| g.clone()) {
                return Ok(entries);
            }
            if let Ok(cached) = load_cache() {
                *self
                    .cache
                    .write()
                    .map_err(|e| format!("cache poisoned: {}", e))? = Some(cached.entries.clone());
                return Ok(cached.entries);
            }
        }

        let entries = scan_start_menu();
        if let Err(e) = save_cache(&entries) {
            warn!("Failed to persist app cache: {}", e);
        }
        *self
            .cache
            .write()
            .map_err(|e| format!("cache poisoned: {}", e))? = Some(entries.clone());
        Ok(entries)
    }
}

impl Default for AppLauncher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for AppLauncher {
    fn name(&self) -> &str {
        "apps.launch"
    }

    fn description(&self) -> &str {
        "Launch a desktop application by name. The query is fuzzy-matched against the user's \
         Start Menu, so partial or approximate names work ('chrome', 'vs code')."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Name or keyword for the application to open."
                },
                "refresh": {
                    "type": "boolean",
                    "description": "Rescan the Start Menu before matching. Use when the user just installed something."
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
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing 'query'"))?
            .to_string();
        let refresh = args
            .get("refresh")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let entries = self
            .ensure_cache(refresh)
            .map_err(|e| ToolError::execution(self.name(), e))?;

        let matched = resolve(&query, &entries).ok_or_else(|| ToolError::Execution {
            tool: self.name().into(),
            reason: format!("no app matched '{}'", query),
        })?;

        launch(&matched.path).map_err(|e| ToolError::execution(self.name(), e))?;
        Ok(ToolResult::new(format!("Launching {}.", matched.name)))
    }
}

/// Alias first, then fuzzy match. Returns a reference to the matching entry.
fn resolve<'a>(query: &str, entries: &'a [AppEntry]) -> Option<&'a AppEntry> {
    let q = query.to_lowercase();

    if let Some((_, canonical)) = ALIASES.iter().find(|(alias, _)| *alias == q) {
        let needle = canonical.to_lowercase();
        if let Some(exact) = entries.iter().find(|e| e.name.to_lowercase() == needle) {
            return Some(exact);
        }
    }

    let mut best: Option<(f64, &AppEntry)> = None;
    for entry in entries {
        let score = jaro_winkler(&q, &entry.name.to_lowercase());
        let better = match best {
            None => true,
            Some((prev, _)) => score > prev,
        };
        if better {
            best = Some((score, entry));
        }
    }
    best.filter(|(score, _)| *score >= MATCH_SCORE_THRESHOLD)
        .map(|(_, entry)| entry)
}

fn scan_start_menu() -> Vec<AppEntry> {
    let mut roots = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        roots.push(PathBuf::from(appdata).join("Microsoft/Windows/Start Menu/Programs"));
    }
    if let Ok(programdata) = std::env::var("ProgramData") {
        roots.push(PathBuf::from(programdata).join("Microsoft/Windows/Start Menu/Programs"));
    }

    let mut out = Vec::new();
    for root in roots {
        collect_shortcuts(&root, &mut out);
    }

    // Deduplicate by lowercase name, keeping the first occurrence.
    let mut seen = std::collections::HashSet::new();
    out.retain(|e| seen.insert(e.name.to_lowercase()));
    out.sort_by(|a, b| a.name.cmp(&b.name));
    debug!("Scanned {} Start Menu entries", out.len());
    out
}

fn collect_shortcuts(dir: &Path, out: &mut Vec<AppEntry>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_shortcuts(&path, out);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !matches!(ext.to_ascii_lowercase().as_str(), "lnk" | "url") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        out.push(AppEntry {
            name: name.to_string(),
            path,
        });
    }
}

fn load_cache() -> Result<AppCache, String> {
    let path = AppLauncher::cache_path()?;
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    serde_json::from_slice::<AppCache>(&bytes).map_err(|e| e.to_string())
}

fn save_cache(entries: &[AppEntry]) -> Result<(), String> {
    let path = AppLauncher::cache_path()?;
    let cache = AppCache {
        scanned_at_unix: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        entries: entries.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&cache).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn launch(path: &Path) -> Result<(), String> {
    // `explorer.exe` resolves `.lnk` and `.url` targets correctly —
    // launching the shortcut directly via CreateProcess would not.
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map_err(|e| format!("failed to launch {}: {}", path.display(), e))?;
    Ok(())
}

#[cfg(not(windows))]
fn launch(_path: &Path) -> Result<(), String> {
    Err("application launching is only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entries() -> Vec<AppEntry> {
        vec![
            AppEntry {
                name: "Google Chrome".into(),
                path: PathBuf::from("chrome.lnk"),
            },
            AppEntry {
                name: "Visual Studio Code".into(),
                path: PathBuf::from("code.lnk"),
            },
            AppEntry {
                name: "Spotify".into(),
                path: PathBuf::from("spotify.lnk"),
            },
        ]
    }

    #[test]
    fn alias_wins_over_fuzzy() {
        let entries = sample_entries();
        let hit = resolve("vscode", &entries).expect("should match vscode alias");
        assert_eq!(hit.name, "Visual Studio Code");
    }

    #[test]
    fn fuzzy_match_returns_best_hit() {
        let entries = sample_entries();
        let hit = resolve("spotfy", &entries).expect("should fuzzy-match Spotify");
        assert_eq!(hit.name, "Spotify");
    }

    #[test]
    fn below_threshold_returns_none() {
        let entries = sample_entries();
        assert!(resolve("xyzzy completely unrelated", &entries).is_none());
    }
}
