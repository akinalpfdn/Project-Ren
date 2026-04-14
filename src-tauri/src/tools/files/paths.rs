//! Shared path-resolution + sandboxing for every file tool.
//!
//! The LLM is **not** allowed to hand us arbitrary absolute paths. Every
//! tool takes a `folder` enum (Downloads / Documents / Desktop / Pictures /
//! Music / Videos) and an optional relative `subpath`. We canonicalise the
//! result and bail if it climbs above the chosen root (`..` traversal,
//! drive-letter switching, junctions to elsewhere). This keeps the read
//! surface to the user's own files.

use std::path::{Path, PathBuf};

/// Folder identifiers the LLM may pass. Mirrors `tools::files::folders`.
pub const ALLOWED_FOLDERS: &[&str] = &[
    "downloads",
    "documents",
    "desktop",
    "pictures",
    "music",
    "videos",
];

pub fn folder_root(key: &str) -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE").ok().map(PathBuf::from)?;
    let suffix = match key {
        "downloads" => "Downloads",
        "documents" => "Documents",
        "desktop" => "Desktop",
        "pictures" => "Pictures",
        "music" => "Music",
        "videos" => "Videos",
        _ => return None,
    };
    Some(home.join(suffix))
}

#[derive(Debug)]
pub enum ResolveError {
    UnknownFolder(String),
    OutsideRoot,
    NotFound(PathBuf),
    Io(std::io::Error),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::UnknownFolder(k) => write!(f, "unknown folder '{}'", k),
            ResolveError::OutsideRoot => write!(
                f,
                "subpath escapes its allowed folder root (likely a '..' traversal)"
            ),
            ResolveError::NotFound(p) => write!(f, "path not found: {}", p.display()),
            ResolveError::Io(e) => write!(f, "io error: {}", e),
        }
    }
}

impl From<std::io::Error> for ResolveError {
    fn from(e: std::io::Error) -> Self {
        ResolveError::Io(e)
    }
}

/// Resolves `<folder_root>/<subpath>`, canonicalises it, and verifies the
/// result still lives inside the resolved folder root.
pub fn resolve(folder: &str, subpath: Option<&str>) -> Result<PathBuf, ResolveError> {
    let key = folder.trim().to_ascii_lowercase();
    if !ALLOWED_FOLDERS.contains(&key.as_str()) {
        return Err(ResolveError::UnknownFolder(folder.to_string()));
    }

    let root = folder_root(&key).ok_or_else(|| ResolveError::UnknownFolder(folder.to_string()))?;
    let candidate = match subpath.map(str::trim).filter(|s| !s.is_empty()) {
        Some(rel) => root.join(rel),
        None => root.clone(),
    };

    if !candidate.exists() {
        return Err(ResolveError::NotFound(candidate));
    }

    let canon_root = std::fs::canonicalize(&root)?;
    let canon_target = std::fs::canonicalize(&candidate)?;
    if !canon_target.starts_with(&canon_root) {
        return Err(ResolveError::OutsideRoot);
    }
    Ok(canon_target)
}

/// Render a path back as a friendly `<Folder>/<sub/path>` string for the
/// LLM to narrate (avoid leaking the user's full home path).
pub fn display_relative(folder_key: &str, full: &Path) -> String {
    let nice_root = match folder_key {
        "downloads" => "Downloads",
        "documents" => "Documents",
        "desktop" => "Desktop",
        "pictures" => "Pictures",
        "music" => "Music",
        "videos" => "Videos",
        _ => folder_key,
    };
    let root = folder_root(folder_key)
        .and_then(|p| std::fs::canonicalize(&p).ok());
    if let Some(root) = root {
        if let Ok(rel) = full.strip_prefix(&root) {
            let rel_str = rel.to_string_lossy();
            if rel_str.is_empty() {
                return nice_root.to_string();
            }
            return format!("{}/{}", nice_root, rel_str.replace('\\', "/"));
        }
    }
    full.display().to_string()
}
