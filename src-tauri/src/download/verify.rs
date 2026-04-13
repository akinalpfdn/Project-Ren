use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use tracing::info;

/// Verify a file's SHA256 hash against an expected hex string.
/// Returns `Ok(())` if the hash matches, `Err` otherwise.
pub fn verify_sha256(path: &Path, expected_hex: &str) -> Result<()> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read file for verification: {}", path.display()))?;

    let computed = hex::encode(Sha256::digest(&bytes));

    if computed.to_lowercase() != expected_hex.to_lowercase() {
        anyhow::bail!(
            "Hash mismatch for {}.\n  Expected: {}\n  Got:      {}",
            path.display(),
            expected_hex,
            computed
        );
    }

    info!("Hash verified: {}", path.display());
    Ok(())
}

/// Check if a file exists AND its hash matches. Used to detect valid cached downloads.
pub fn is_valid_download(path: &Path, expected_hex: &str) -> bool {
    if !path.exists() {
        return false;
    }
    verify_sha256(path, expected_hex).is_ok()
}
