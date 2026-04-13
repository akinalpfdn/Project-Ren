//! `steam.launch` — fuzzy-match a user query against installed Steam games
//! and launch the best hit via the `steam://rungameid/<appid>` protocol.
//!
//! The game list is scanned on first use and cached to disk; `refresh=true`
//! forces a rescan for users who just installed a title.

use std::sync::RwLock;

use async_trait::async_trait;
use serde_json::{json, Value};
use strsim::jaro_winkler;
use tracing::warn;

use crate::tools::steam::library::{self, SteamGame};
use crate::tools::{Tool, ToolError, ToolResult};

/// Jaro-Winkler score below which a match is considered unreliable.
const MATCH_SCORE_THRESHOLD: f64 = 0.80;

pub struct SteamLauncher {
    cache: RwLock<Option<Vec<SteamGame>>>,
}

impl SteamLauncher {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(None),
        }
    }

    fn ensure_cache(&self, refresh: bool) -> Result<Vec<SteamGame>, String> {
        if !refresh {
            if let Some(games) = self.cache.read().ok().and_then(|g| g.clone()) {
                return Ok(games);
            }
            if let Ok(cached) = library::load_cache() {
                *self
                    .cache
                    .write()
                    .map_err(|e| format!("cache poisoned: {}", e))? = Some(cached.games.clone());
                return Ok(cached.games);
            }
        }

        let games = library::scan_games();
        if let Err(e) = library::save_cache(&games) {
            warn!("Failed to persist Steam cache: {}", e);
        }
        *self
            .cache
            .write()
            .map_err(|e| format!("cache poisoned: {}", e))? = Some(games.clone());
        Ok(games)
    }
}

impl Default for SteamLauncher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SteamLauncher {
    fn name(&self) -> &str {
        "steam.launch"
    }

    fn description(&self) -> &str {
        "Launch a game installed through Steam by its title. The query is fuzzy-matched \
         against installed titles, so partial or approximate names work."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Name or keyword for the Steam game to launch."
                },
                "refresh": {
                    "type": "boolean",
                    "description": "Rescan installed games before matching. Use when the user just installed a title."
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

        let games = self
            .ensure_cache(refresh)
            .map_err(|e| ToolError::execution(self.name(), e))?;

        if games.is_empty() {
            return Err(ToolError::execution(
                self.name(),
                "no Steam games found — is Steam installed?",
            ));
        }

        let matched = resolve(&query, &games).ok_or_else(|| ToolError::Execution {
            tool: self.name().into(),
            reason: format!("no Steam game matched '{}'", query),
        })?;

        launch(&matched.appid).map_err(|e| ToolError::execution(self.name(), e))?;
        Ok(ToolResult::new(format!("Launching {} on Steam.", matched.name)))
    }
}

fn resolve<'a>(query: &str, games: &'a [SteamGame]) -> Option<&'a SteamGame> {
    let q = query.to_lowercase();
    let mut best: Option<(f64, &SteamGame)> = None;
    for game in games {
        let score = jaro_winkler(&q, &game.name.to_lowercase());
        let better = match best {
            None => true,
            Some((prev, _)) => score > prev,
        };
        if better {
            best = Some((score, game));
        }
    }
    best.filter(|(score, _)| *score >= MATCH_SCORE_THRESHOLD)
        .map(|(_, game)| game)
}

#[cfg(windows)]
fn launch(appid: &str) -> Result<(), String> {
    // `cmd /C start ""` hands the URI off to the default protocol handler
    // without leaving a visible shell window.
    let uri = format!("steam://rungameid/{}", appid);
    std::process::Command::new("cmd")
        .args(["/C", "start", "", &uri])
        .spawn()
        .map_err(|e| format!("failed to launch {}: {}", uri, e))?;
    Ok(())
}

#[cfg(not(windows))]
fn launch(_appid: &str) -> Result<(), String> {
    Err("Steam launching is only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_games() -> Vec<SteamGame> {
        vec![
            SteamGame {
                appid: "730".into(),
                name: "Counter-Strike 2".into(),
            },
            SteamGame {
                appid: "570".into(),
                name: "Dota 2".into(),
            },
            SteamGame {
                appid: "1091500".into(),
                name: "Cyberpunk 2077".into(),
            },
        ]
    }

    #[test]
    fn exact_title_matches() {
        let games = sample_games();
        let hit = resolve("Dota 2", &games).expect("exact match");
        assert_eq!(hit.appid, "570");
    }

    #[test]
    fn partial_title_matches() {
        let games = sample_games();
        let hit = resolve("cyberpunk", &games).expect("partial match");
        assert_eq!(hit.appid, "1091500");
    }

    #[test]
    fn unrelated_query_returns_none() {
        let games = sample_games();
        assert!(resolve("totally unrelated text", &games).is_none());
    }
}
