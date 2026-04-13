# Phase 06 — Calendar, Weather, Media, Spotify
Status: PENDING (depends on Phase 5)

## Goal
Ren becomes truly useful as a daily assistant with context awareness and media control.

## Context From Phase 5
- Tool system is live. `ToolRegistry`, `Tool` trait, and `ToolCard` UI all working.
- `weather.rs` stub may already exist from Phase 5 Web category — if so, this phase makes it fully configurable and moves it to its own category.
- `AppConfig` in `src-tauri/src/config/mod.rs` already stores `location` and `brave_api_key`. In Phase 6 it gains `spotify_token`, `google_refresh_token`.
- OAuth tokens must be stored encrypted in `%APPDATA%\Ren\credentials.db` — NOT in `config.json` (which may be world-readable).

## Tasks

### Credentials Store
- [ ] `CredentialStore` in `src-tauri/src/storage/credentials.rs`: encrypts/decrypts OAuth tokens using `ring` or `aes-gcm` crate with a machine-derived key (e.g. Windows DPAPI via `windows-rs`)
- [ ] Store path: `%APPDATA%\Ren\credentials.db` (simple encrypted JSON file — no SQLite needed at this scale)
- [ ] Methods: `save(key, value)`, `load(key) -> Option<String>`, `delete(key)`

### Weather (Full Implementation)
- [ ] Move from `tools/web/weather.rs` to `tools/weather/mod.rs` (own category)
- [ ] Open-Meteo: geocoding API to resolve location name to lat/lon; then forecast API for current + today's summary
- [ ] Location: from `AppConfig.location` (set in settings panel) OR extracted from LLM tool call params (user says "weather in Ankara")
- [ ] Format response as plain English for LLM to narrate: "Currently 18°C and partly cloudy in Istanbul. High of 22°C today."

### Google Calendar Integration
- [ ] OAuth2 PKCE flow: open browser to Google auth URL via `opener`, listen on localhost redirect
- [ ] `GoogleCalendarClient` in `src-tauri/src/tools/calendar/google.rs`: wraps `reqwest`, uses stored refresh token, auto-refresh on expiry
- [ ] Tool: `get_today_events` — return list of events with time and title
- [ ] Tool: `get_upcoming_events` — return next N events (configurable, default 5)
- [ ] `CalendarTrait` in `src-tauri/src/tools/calendar/mod.rs` — abstracted for future providers (iCal, Outlook)

### Windows Media Transport Controls
- [ ] `MediaController` in `src-tauri/src/tools/media/windows_media.rs`: wraps WinRT `GlobalSystemMediaTransportControlsSessionManager` via `windows-rs`
- [ ] Tools: `play`, `pause`, `next_track`, `previous_track`, `set_volume`
- [ ] Works universally against whatever media app is active (Spotify, browser YouTube, VLC, etc.)
- [ ] Current track info: expose `get_current_track()` so Ren can say "currently playing X by Y"

### Spotify Integration
- [ ] OAuth2 PKCE flow (no client secret needed): open browser, capture redirect, store tokens via `CredentialStore`
- [ ] `SpotifyClient` in `src-tauri/src/tools/spotify/client.rs`: wraps `rspotify` crate (or direct `reqwest`)
- [ ] Tools: `search_and_play(query)`, `play_playlist(name)`, `play_artist(name)`, `get_current_playback`
- [ ] Graceful fallback: if user not logged in or not Premium, fall back to Windows Media Transport Controls with an explanation
- [ ] Rate limiting: Spotify Web API rate limits per app. Implement exponential backoff. Log warnings if limits are being approached.

### Settings Panel (Frontend)
- [ ] Settings component in `src/components/Settings.tsx` + `Settings.module.css`
- [ ] Toggled from tray menu ("Settings") or hotkey
- [ ] Sections: Microphone, Sensitivity, API Keys (Brave), Connections (Spotify, Google Calendar), Location, Autostart
- [ ] "Connect Spotify" button: triggers OAuth flow via Tauri command, shows connected status + account name after auth
- [ ] "Connect Google Calendar" button: same pattern
- [ ] "Disconnect" option for each connection
- [ ] All settings write to `AppConfig` which persists to `%APPDATA%\Ren\config.json`

### Frontend
- [ ] Tauri commands for OAuth flows: `connect_spotify()`, `connect_google_calendar()` — these open browser, handle redirect, store tokens, return success/failure
- [ ] Settings panel slides in from the right or appears as an overlay — animated with Framer Motion
- [ ] Connection status indicators: green dot (connected) / grey dot (disconnected) next to each service

## Architecture Notes

### File Structure Additions
```
src-tauri/src/
  storage/
    mod.rs
    credentials.rs    — CredentialStore (encrypted)
  tools/
    calendar/
      mod.rs          — CalendarTrait
      google.rs       — GoogleCalendarClient
    media/
      mod.rs
      windows_media.rs — WinRT SMTC wrapper
    spotify/
      mod.rs
      client.rs       — SpotifyClient
    weather/          — moved from tools/web/
      mod.rs
```

### OAuth Flow Pattern (same for both Google and Spotify)
1. Rust command opens browser to OAuth URL
2. Rust starts local HTTP server on `localhost:8765` waiting for redirect
3. User authorizes in browser, browser redirects to `localhost:8765/callback?code=...`
4. Rust extracts code, exchanges for tokens, stores via `CredentialStore`
5. Tauri command returns `Ok(())` to frontend, frontend shows "Connected" status
6. Access token refresh happens transparently on each API call

### New Config Keys
```json
{
  "location": "Istanbul",
  "brave_api_key": "...",
  "autostart": false,
  "conversation_timeout_secs": 30,
  "wake_sensitivity": 0.5,
  "tts_voice": "bf_emma"
}
```
Note: OAuth tokens are NOT stored in config.json — they go in credentials.db.

### New Tauri Events
- `ren://oauth-status` — payload: `{ service: "spotify" | "google_calendar", connected: boolean, account?: string }`

## Acceptance Criteria
- [ ] "Ren, what's on my calendar today" reads events via Google Calendar
- [ ] "Ren, pause the music" works regardless of which media app is playing
- [ ] "Ren, play Radiohead on Spotify" starts playback for Premium users
- [ ] OAuth flows complete entirely within Ren UI — no command line, no manual token pasting
- [ ] Credentials survive Ren restart
- [ ] Disconnect/reconnect works cleanly
- [ ] Settings panel opens, all connections visible, all settings persistent

## Decisions Made This Phase
<!-- Append here as they happen -->

## Known Risks
- Windows DPAPI for key derivation: ties encryption key to the Windows user account. If user migrates machines, credentials cannot be transferred. Document this in settings UI ("credentials are tied to this device").
- Spotify shared app rate limits: all Ren users share one Spotify app registration. At scale this could be a problem. Noted for future; at indie scale it's fine.
- Google Calendar OAuth: requires registering a Google Cloud project and embedding OAuth client ID (not secret — PKCE). This client ID is visible in the binary; document this openly.
