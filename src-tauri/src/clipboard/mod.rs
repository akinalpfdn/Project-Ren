//! Minimal Windows-clipboard reader used by the Phase 8.2 "clipboard context"
//! hotkey. We only ever read UTF-16 text (`CF_UNICODETEXT`) on demand — never
//! poll, never write — so the surface stays tiny and the permission story
//! stays honest: Ren only touches the clipboard when the user presses the
//! capture hotkey.

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};

/// Shared "armed clipboard preamble" managed as Tauri state. `Some` means the
/// next user turn will be wrapped with the captured text; `None` is the
/// default cold state. Cleared automatically after one turn or via the
/// `clear_clipboard_arm` command.
pub type SharedClipboardArm = Arc<Mutex<Option<String>>>;

/// Convenience for the setup closure — keeps the type alias as the only
/// thing other modules need to import.
pub fn new_arm() -> SharedClipboardArm {
    Arc::new(Mutex::new(None))
}

/// Truncates the captured payload to a short preview suitable for emitting
/// to the frontend badge — never log full clipboard contents.
pub fn preview_of(text: &str) -> String {
    const MAX: usize = 80;
    let trimmed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.chars().count() <= MAX {
        trimmed
    } else {
        let mut out: String = trimmed.chars().take(MAX).collect();
        out.push('…');
        out
    }
}

#[cfg(windows)]
pub fn read_text() -> Result<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    /// Defensive cap on how many UTF-16 code units we walk before giving up,
    /// in case the clipboard payload is missing its NUL terminator.
    const MAX_CODE_UNITS: usize = 10_000_000;

    unsafe {
        OpenClipboard(None).context("OpenClipboard failed")?;

        let result = (|| -> Result<String> {
            let handle = GetClipboardData(CF_UNICODETEXT.0.into())
                .map_err(|e| anyhow!("GetClipboardData failed: {}", e))?;
            if handle.0.is_null() {
                return Err(anyhow!("Clipboard has no Unicode text right now"));
            }

            let global = HGLOBAL(handle.0);
            let locked = GlobalLock(global);
            if locked.is_null() {
                return Err(anyhow!("GlobalLock returned null"));
            }

            let ptr = locked as *const u16;
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
                if len > MAX_CODE_UNITS {
                    return Err(anyhow!("Clipboard payload exceeds safety limit"));
                }
            }
            let slice = std::slice::from_raw_parts(ptr, len);
            let text = String::from_utf16_lossy(slice);

            let _ = GlobalUnlock(global);
            Ok(text)
        })();

        let _ = CloseClipboard();
        result
    }
}

#[cfg(not(windows))]
pub fn read_text() -> Result<String> {
    Err(anyhow!("Clipboard access is only supported on Windows"))
}
