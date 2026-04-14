//! Thin helper around the Windows Runtime `GlobalSystemMediaTransportControls*`
//! classes. Everything here blocks on `IAsyncOperation::get()`, so callers are
//! expected to wrap invocations in `spawn_blocking`.
//!
//! Why a layer at all: the windows-rs call-sites are noisy (every operation
//! goes through an async factory, `.get()`, `.ok()?`, HSTRING → Rust String),
//! and the `Tool` impls read much cleaner when they can just call
//! `play()` / `pause()` / `try_current_track()` and be done with it.

use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

fn current_session() -> Result<
    windows::Media::Control::GlobalSystemMediaTransportControlsSession,
    String,
> {
    let manager_op = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|e| format!("SMTC manager factory failed: {}", e))?;
    let manager = manager_op
        .get()
        .map_err(|e| format!("SMTC manager await failed: {}", e))?;
    manager
        .GetCurrentSession()
        .map_err(|_| "No active media session — start playback first.".to_string())
}

pub fn play() -> Result<(), String> {
    let session = current_session()?;
    let op = session
        .TryPlayAsync()
        .map_err(|e| format!("TryPlayAsync failed: {}", e))?;
    let ok = op
        .get()
        .map_err(|e| format!("TryPlayAsync await failed: {}", e))?;
    if !ok {
        return Err("Current media session refused to resume.".into());
    }
    Ok(())
}

pub fn pause() -> Result<(), String> {
    let session = current_session()?;
    let op = session
        .TryPauseAsync()
        .map_err(|e| format!("TryPauseAsync failed: {}", e))?;
    let ok = op
        .get()
        .map_err(|e| format!("TryPauseAsync await failed: {}", e))?;
    if !ok {
        return Err("Current media session refused to pause.".into());
    }
    Ok(())
}

pub fn next_track() -> Result<(), String> {
    let session = current_session()?;
    let op = session
        .TrySkipNextAsync()
        .map_err(|e| format!("TrySkipNextAsync failed: {}", e))?;
    let ok = op
        .get()
        .map_err(|e| format!("TrySkipNextAsync await failed: {}", e))?;
    if !ok {
        return Err("Current media session refused to skip.".into());
    }
    Ok(())
}

pub fn previous_track() -> Result<(), String> {
    let session = current_session()?;
    let op = session
        .TrySkipPreviousAsync()
        .map_err(|e| format!("TrySkipPreviousAsync failed: {}", e))?;
    let ok = op
        .get()
        .map_err(|e| format!("TrySkipPreviousAsync await failed: {}", e))?;
    if !ok {
        return Err("Current media session refused to go back.".into());
    }
    Ok(())
}

pub fn try_current_track() -> Result<String, String> {
    let session = match current_session() {
        Ok(s) => s,
        Err(_) => return Ok("Nothing is currently playing.".into()),
    };

    let props_op = session
        .TryGetMediaPropertiesAsync()
        .map_err(|e| format!("TryGetMediaPropertiesAsync failed: {}", e))?;
    let props = props_op
        .get()
        .map_err(|e| format!("TryGetMediaPropertiesAsync await failed: {}", e))?;

    let title = props
        .Title()
        .map(|h| h.to_string())
        .unwrap_or_default();
    let artist = props
        .Artist()
        .map(|h| h.to_string())
        .unwrap_or_default();

    Ok(match (title.trim().is_empty(), artist.trim().is_empty()) {
        (true, true) => "Something is playing but the app didn't report track metadata.".into(),
        (false, true) => format!("Currently playing '{}'.", title),
        (true, false) => format!("Currently playing something by {}.", artist),
        (false, false) => format!("Currently playing '{}' by {}.", title, artist),
    })
}
