//! Discovers the local Steam installation and the games installed in it.
//!
//! Steam stores the list of library folders in `steamapps/libraryfolders.vdf`
//! and each installed title in `steamapps/appmanifest_<appid>.acf`. We locate
//! Steam itself via the Windows registry (falling back to common default
//! paths), then parse the manifests into a flat `(appid, name)` list that
//! `SteamLauncher` fuzzy-matches against.
//!
//! The result is cached to `%APPDATA%\Ren\cache\steam_games.json`; a rescan
//! is only required when the user installs or uninstalls a game.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::config;
use crate::tools::steam::vdf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamGame {
    pub appid: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamCache {
    pub scanned_at_unix: u64,
    pub games: Vec<SteamGame>,
}

/// Scans the local machine and returns every installed Steam game.
/// Returns an empty vector (not an error) if Steam is not installed.
pub fn scan_games() -> Vec<SteamGame> {
    let Some(steam_root) = locate_steam_root() else {
        debug!("Steam installation not found");
        return Vec::new();
    };
    debug!("Found Steam root at {}", steam_root.display());

    let mut libraries = vec![steam_root.clone()];
    if let Err(e) = extend_with_additional_libraries(&steam_root, &mut libraries) {
        warn!("Could not read libraryfolders.vdf: {}", e);
    }

    let mut games = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for lib in &libraries {
        let steamapps = lib.join("steamapps");
        collect_manifests(&steamapps, &mut games, &mut seen_ids);
    }
    games.sort_by(|a, b| a.name.cmp(&b.name));
    debug!("Discovered {} Steam games", games.len());
    games
}

/// Persists the scan to disk.
pub fn save_cache(games: &[SteamGame]) -> Result<(), String> {
    let path = cache_path()?;
    let cache = SteamCache {
        scanned_at_unix: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        games: games.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&cache).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

/// Loads a previously saved scan, if present.
pub fn load_cache() -> Result<SteamCache, String> {
    let path = cache_path()?;
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    serde_json::from_slice::<SteamCache>(&bytes).map_err(|e| e.to_string())
}

fn cache_path() -> Result<PathBuf, String> {
    config::cache_dir()
        .map(|p| p.join("steam_games.json"))
        .map_err(|e| e.to_string())
}

/// Returns the Steam install directory (the folder containing `steam.exe`).
fn locate_steam_root() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(p) = read_steam_path_from_registry() {
        if p.join("steam.exe").exists() {
            return Some(p);
        }
    }

    for candidate in default_install_paths() {
        if candidate.join("steam.exe").exists() {
            return Some(candidate);
        }
    }
    None
}

fn default_install_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        out.push(PathBuf::from(pf86).join("Steam"));
    }
    if let Ok(pf) = std::env::var("ProgramFiles") {
        out.push(PathBuf::from(pf).join("Steam"));
    }
    out
}

#[cfg(windows)]
fn read_steam_path_from_registry() -> Option<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_SZ,
    };

    let subkey: Vec<u16> = "Software\\Valve\\Steam\0".encode_utf16().collect();
    let value: Vec<u16> = "SteamPath\0".encode_utf16().collect();

    // SAFETY: All pointers are to locally-owned buffers; the handle is
    // closed in every exit path. `RegGetValueW` fills `buf` with a
    // null-terminated UTF-16 string and writes its size into `len`.
    unsafe {
        let mut hkey = HKEY::default();
        let open = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            Some(0),
            KEY_READ,
            &mut hkey,
        );
        if open != ERROR_SUCCESS {
            return None;
        }

        let mut buf = [0u16; 1024];
        let mut len: u32 = (buf.len() * 2) as u32;
        let read = RegGetValueW(
            hkey,
            PCWSTR::null(),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        );
        let _ = RegCloseKey(hkey);

        if read != ERROR_SUCCESS {
            return None;
        }

        let chars = (len as usize / 2).saturating_sub(1);
        let raw = String::from_utf16_lossy(&buf[..chars]);
        let normalised = raw.replace('/', "\\");
        Some(PathBuf::from(normalised))
    }
}

/// Parses `steamapps/libraryfolders.vdf` to find library roots outside the
/// main Steam install (e.g. an external drive).
fn extend_with_additional_libraries(
    steam_root: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let manifest = steam_root.join("steamapps").join("libraryfolders.vdf");
    if !manifest.exists() {
        return Ok(());
    }

    let text = fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
    let parsed = vdf::parse(&text)?;

    // Post-2021 format: `"libraryfolders" { "0" { "path" "..." } "1" { ... } }`.
    // Older format has the path directly at `"libraryfolders" { "0" "..." }`.
    let Some(folders) = parsed.get("libraryfolders").and_then(|v| v.as_object()) else {
        return Ok(());
    };

    for value in folders.values() {
        let path = match value {
            vdf::Vdf::Object(_) => value.get("path").and_then(|v| v.as_str()),
            vdf::Vdf::String(s) => Some(s.as_str()),
        };
        if let Some(p) = path {
            let normalised = p.replace("\\\\", "\\");
            let buf = PathBuf::from(normalised);
            if !out.iter().any(|existing| existing == &buf) {
                out.push(buf);
            }
        }
    }
    Ok(())
}

/// Reads every `appmanifest_*.acf` in the given `steamapps` directory.
fn collect_manifests(
    steamapps: &Path,
    out: &mut Vec<SteamGame>,
    seen_ids: &mut std::collections::HashSet<String>,
) {
    let Ok(entries) = fs::read_dir(steamapps) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
            continue;
        }
        match parse_manifest(&path) {
            Ok(game) => {
                if seen_ids.insert(game.appid.clone()) {
                    out.push(game);
                }
            }
            Err(e) => warn!("Skipping {}: {}", path.display(), e),
        }
    }
}

fn parse_manifest(path: &Path) -> Result<SteamGame, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let parsed = vdf::parse(&text)?;
    let app = parsed
        .get("AppState")
        .ok_or_else(|| "missing AppState".to_string())?;
    let appid = app
        .get("appid")
        .and_then(vdf::Vdf::as_str)
        .ok_or_else(|| "missing appid".to_string())?
        .to_string();
    let name = app
        .get("name")
        .and_then(vdf::Vdf::as_str)
        .ok_or_else(|| "missing name".to_string())?
        .to_string();
    Ok(SteamGame { appid, name })
}
