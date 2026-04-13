pub mod verify;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use crate::config::defaults::DOWNLOAD_CHUNK_SIZE;

/// Progress event payload sent to the frontend.
#[derive(Clone, Serialize)]
pub struct DownloadProgressPayload {
    /// Logical name of what's being downloaded (e.g. "whisper", "ollama_bin", "kokoro", "qwen")
    pub step: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    /// Bytes per second (rolling average)
    pub speed_bps: u64,
}

/// Downloads a file from `url` to `dest`, resuming if a partial file exists.
/// Emits `ren://download-progress` events through `app`.
/// Verifies SHA256 after a complete download if `expected_sha256` is provided.
pub async fn download_file(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    step: &str,
    expected_sha256: Option<&str>,
) -> Result<()> {
    // Check if already complete and valid
    if let Some(hash) = expected_sha256 {
        if verify::is_valid_download(dest, hash) {
            info!("Already downloaded and verified: {}", dest.display());
            return Ok(());
        }
    } else if dest.exists() {
        info!("Already exists (no hash check): {}", dest.display());
        return Ok(());
    }

    // Create parent directory
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let partial_path = partial_file_path(dest);
    let existing_bytes = partial_size(&partial_path).await;

    let client = reqwest::Client::new();
    let mut request = client.get(url);

    // Resume download if we have a partial file
    if existing_bytes > 0 {
        info!("Resuming download from byte {}", existing_bytes);
        request = request.header("Range", format!("bytes={}-", existing_bytes));
    }

    let response = request.send().await
        .with_context(|| format!("HTTP request failed for {}", url))?;

    if !response.status().is_success()
        && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
    {
        anyhow::bail!(
            "Download failed: HTTP {} for {}",
            response.status(),
            url
        );
    }

    let total_bytes = response
        .content_length()
        .map(|l| l + existing_bytes)
        .unwrap_or(0);

    info!("Downloading {} → {} ({} bytes total)", url, dest.display(), total_bytes);

    // Append to partial file
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&partial_path)
        .await
        .with_context(|| format!("Failed to open partial file: {}", partial_path.display()))?;

    let mut downloaded = existing_bytes;
    let mut stream = response.bytes_stream();
    let mut speed_window: Vec<(std::time::Instant, u64)> = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Error reading download chunk")?;
        file.write_all(&chunk).await
            .context("Failed to write chunk to disk")?;

        downloaded += chunk.len() as u64;

        // Rolling speed estimate
        let now = std::time::Instant::now();
        speed_window.push((now, chunk.len() as u64));
        speed_window.retain(|(t, _)| now.duration_since(*t).as_secs() < 3);
        let speed_bps: u64 = speed_window.iter().map(|(_, b)| b).sum::<u64>()
            / speed_window.len().max(1) as u64;

        let _ = app.emit(
            "ren://download-progress",
            DownloadProgressPayload {
                step: step.to_string(),
                downloaded_bytes: downloaded,
                total_bytes,
                speed_bps,
            },
        );
    }

    file.flush().await.context("Failed to flush download file")?;
    drop(file);

    // Rename partial → final
    tokio::fs::rename(&partial_path, dest).await
        .with_context(|| format!(
            "Failed to rename {} → {}",
            partial_path.display(),
            dest.display()
        ))?;

    info!("Download complete: {}", dest.display());

    // Verify hash if provided
    if let Some(hash) = expected_sha256 {
        verify::verify_sha256(dest, hash)
            .context("Downloaded file failed hash verification — file may be corrupt")?;
    }

    Ok(())
}

fn partial_file_path(dest: &Path) -> PathBuf {
    let mut p = dest.to_path_buf();
    let name = p
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    p.set_file_name(format!("{}.part", name));
    p
}

async fn partial_size(path: &Path) -> u64 {
    tokio::fs::metadata(path)
        .await
        .map(|m| m.len())
        .unwrap_or(0)
}
