//! Encrypted credential store backed by the Windows Data Protection API
//! (DPAPI) under the caller's user scope.
//!
//! We keep the persisted on-disk format intentionally boring: one JSON object
//! `{ key: value }` for the plaintext, DPAPI-encrypted as a single blob, then
//! base64-encoded so the resulting `credentials.db` stays inspectable with a
//! text editor without leaking secret material. Whole-blob encryption (vs.
//! per-value) also hides *which* services are connected from anyone casually
//! poking at the file.
//!
//! The DPAPI key is derived by Windows from the current user + machine; no
//! secret ships with the binary. Tradeoff: the store cannot be migrated
//! across machines or user accounts, which is surfaced in the settings UI.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};

use crate::config::app_data_dir;

const CREDENTIAL_STORE_FILENAME: &str = "credentials.db";

/// On-disk envelope — only the ciphertext lives in `payload`. The empty map
/// case still produces a valid (encrypted) envelope so we can distinguish
/// "never written" from "intentionally empty".
#[derive(Serialize, Deserialize)]
struct Envelope {
    /// DPAPI ciphertext of the serialized `HashMap<String, String>`.
    payload: String,
}

/// User-scoped encrypted key/value store.
pub struct CredentialStore {
    path: PathBuf,
}

impl CredentialStore {
    /// Opens (or lazily creates) the store at `%APPDATA%\Ren\credentials.db`.
    pub fn open() -> Result<Self> {
        let path = app_data_dir()?.join(CREDENTIAL_STORE_FILENAME);
        Ok(Self { path })
    }

    /// Persists `value` under `key`. Overwrites any existing entry for the
    /// same key.
    pub fn save(&self, key: &str, value: &str) -> Result<()> {
        let mut map = self.read_all()?;
        map.insert(key.to_string(), value.to_string());
        self.write_all(&map)
    }

    /// Returns the value for `key` if the store has one, `None` otherwise.
    pub fn load(&self, key: &str) -> Result<Option<String>> {
        let map = self.read_all()?;
        Ok(map.get(key).cloned())
    }

    /// Removes `key` if present. No-op when the key is missing so callers can
    /// blindly "disconnect" a service.
    pub fn delete(&self, key: &str) -> Result<()> {
        let mut map = self.read_all()?;
        if map.remove(key).is_some() {
            self.write_all(&map)?;
        }
        Ok(())
    }

    fn read_all(&self) -> Result<HashMap<String, String>> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }
        let raw = std::fs::read_to_string(&self.path)
            .with_context(|| format!("Failed to read {}", self.path.display()))?;
        if raw.trim().is_empty() {
            return Ok(HashMap::new());
        }
        let envelope: Envelope = serde_json::from_str(&raw)
            .context("Credential store is corrupt (invalid JSON envelope)")?;
        let ciphertext = BASE64
            .decode(envelope.payload.as_bytes())
            .context("Credential payload is not valid base64")?;
        let plaintext = dpapi::unprotect(&ciphertext)
            .context("Failed to decrypt credential store (DPAPI)")?;
        let map: HashMap<String, String> = serde_json::from_slice(&plaintext)
            .context("Decrypted credential payload is not valid JSON")?;
        Ok(map)
    }

    fn write_all(&self, map: &HashMap<String, String>) -> Result<()> {
        let plaintext = serde_json::to_vec(map)
            .context("Failed to serialize credentials")?;
        let ciphertext = dpapi::protect(&plaintext)
            .context("Failed to encrypt credential store (DPAPI)")?;
        let envelope = Envelope {
            payload: BASE64.encode(&ciphertext),
        };
        let encoded = serde_json::to_vec_pretty(&envelope)
            .context("Failed to serialize credential envelope")?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create credential store directory {}", parent.display())
            })?;
        }
        std::fs::write(&self.path, encoded)
            .with_context(|| format!("Failed to write {}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(windows)]
mod dpapi {
    use anyhow::{anyhow, Result};
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };

    /// Encrypts `plaintext` with DPAPI user scope and returns the ciphertext
    /// bytes. The ciphertext embeds every parameter needed to decrypt later.
    pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: plaintext.len() as u32,
            pbData: plaintext.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();

        unsafe {
            CryptProtectData(
                &input,
                PWSTR::null(),
                None,
                None,
                None,
                0,
                &mut output,
            )
            .map_err(|e| anyhow!("CryptProtectData failed: {}", e))?;

            let result = copy_blob_out(&output);
            free_blob(&output);
            Ok(result)
        }
    }

    /// Decrypts DPAPI ciphertext produced by `protect()`.
    pub fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: ciphertext.len() as u32,
            pbData: ciphertext.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();

        unsafe {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                0,
                &mut output,
            )
            .map_err(|e| anyhow!("CryptUnprotectData failed: {}", e))?;

            let result = copy_blob_out(&output);
            free_blob(&output);
            Ok(result)
        }
    }

    unsafe fn copy_blob_out(blob: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if blob.pbData.is_null() || blob.cbData == 0 {
            return Vec::new();
        }
        std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec()
    }

    unsafe fn free_blob(blob: &CRYPT_INTEGER_BLOB) {
        if !blob.pbData.is_null() {
            let _ = LocalFree(Some(HLOCAL(blob.pbData as *mut _)));
        }
    }
}

#[cfg(not(windows))]
mod dpapi {
    use anyhow::{bail, Result};

    pub fn protect(_plaintext: &[u8]) -> Result<Vec<u8>> {
        bail!("Credential store is only supported on Windows (DPAPI)")
    }

    pub fn unprotect(_ciphertext: &[u8]) -> Result<Vec<u8>> {
        bail!("Credential store is only supported on Windows (DPAPI)")
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    // Each test gets its own file so parallel execution can't clobber state.
    // The DPAPI key is still derived from the current user, so tests exercise
    // the real protect/unprotect path without talking to the user's production
    // credential file.
    fn isolated_store() -> CredentialStore {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let tmp = env::temp_dir().join(format!(
            "ren-test-{}-{}",
            std::process::id(),
            id
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        CredentialStore {
            path: tmp.join("credentials.db"),
        }
    }

    #[test]
    fn save_and_load_round_trips() {
        let store = isolated_store();
        store.save("service.token", "hunter2").unwrap();
        assert_eq!(
            store.load("service.token").unwrap().as_deref(),
            Some("hunter2")
        );
    }

    #[test]
    fn missing_key_returns_none() {
        let store = isolated_store();
        assert!(store.load("never-set").unwrap().is_none());
    }

    #[test]
    fn delete_removes_entry() {
        let store = isolated_store();
        store.save("tmp", "v").unwrap();
        store.delete("tmp").unwrap();
        assert!(store.load("tmp").unwrap().is_none());
    }

    #[test]
    fn overwrite_replaces_value() {
        let store = isolated_store();
        store.save("k", "v1").unwrap();
        store.save("k", "v2").unwrap();
        assert_eq!(store.load("k").unwrap().as_deref(), Some("v2"));
    }
}
