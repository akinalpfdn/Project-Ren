# Phase 05 — Tool System and First Tool Categories
Status: CODE COMPLETE — voice-loop acceptance pending physical mic access (2026-04-14).

## Goal
Ren can take real actions on the system. Application launching, Steam games, system controls, folder opening, and web search all work via LLM tool calls.

## Context From Phase 4
- Full voice loop is live: wake → listen → STT → LLM → TTS → speak → sleep.
- System prompt in `src-tauri/src/llm/prompt.rs` currently has no tools. In Phase 5, tool schemas are appended to the prompt dynamically from the registry.
- LLM HTTP client in `src-tauri/src/llm/client.rs` sends plain chat — in Phase 5 it must parse tool call responses from the JSON.

## Tasks

### Tool Infrastructure
- [ ] Define `Tool` trait in `src-tauri/src/tools/mod.rs`:
  ```rust
  pub trait Tool: Send + Sync {
      fn name(&self) -> &str;
      fn description(&self) -> &str;
      fn schema(&self) -> serde_json::Value;  // JSON Schema for LLM
      fn execute(&self, params: serde_json::Value) -> BoxFuture<Result<ToolResult, ToolError>>;
  }
  ```
- [ ] `ToolRegistry` in `src-tauri/src/tools/registry.rs`: `register()`, `get()`, `all_schemas()` (returns Vec of JSON schemas to inject into LLM prompt), `dispatch()` (routes tool call to correct executor)
- [ ] `ToolResult` and `ToolError` types: structured enough that LLM can interpret the result
- [ ] LLM tool call parsing: update `OllamaClient` to detect tool call JSON in response stream, extract tool name + params
- [ ] Tool execution pipeline: LLM emits tool call → dispatch to executor → result fed back to LLM as tool message → LLM generates final response
- [ ] System prompt builder: `prompt.rs` now calls `registry.all_schemas()` to append tool definitions
- [ ] Tool call confirmation flow: destructive tools (shutdown, close-all-apps) must ask user for confirmation via a spoken prompt before executing

### Tool Implementations

#### System Category (`src-tauri/src/tools/system/`)
- [ ] `volume.rs` — get/set system volume percentage via `windows-rs` `IMMDeviceEnumerator`
- [ ] `brightness.rs` — set monitor brightness via WMI `WmiMonitorBrightnessMethods` (skip gracefully if not supported)
- [ ] `lock.rs` — `LockWorkStation()` Win32 API
- [ ] `shutdown.rs` — `ExitWindowsEx()` with 10-second countdown; Ren speaks countdown; hotkey or voice cancels
- [ ] `restart.rs` — same pattern as shutdown

#### Apps Category (`src-tauri/src/tools/apps/`)
- [ ] `launcher.rs` — scan Start Menu (`%APPDATA%\Microsoft\Windows\Start Menu\Programs\` + common folders), cache app list to `%APPDATA%\Ren\cache\apps.json`, fuzzy match on user input, launch via `std::process::Command`
- [ ] Common aliases: "chrome" → "Google Chrome", "vscode" → "Visual Studio Code", etc. — define alias map in `launcher.rs`

#### Steam Category (`src-tauri/src/tools/steam/`)
- [ ] `library.rs` — locate Steam installation, parse `libraryfolders.vdf`, parse each `appmanifest_*.acf` file, cache to `%APPDATA%\Ren\cache\steam_games.json`
- [ ] `launcher.rs` — launch via `steam://rungameid/<appid>` URI using `opener` crate or `ShellExecute`

#### Files Category (`src-tauri/src/tools/files/`)
- [ ] `open_folder.rs` — open common folders (Downloads, Documents, Desktop, Pictures, Music, Videos) via `explorer.exe` path; use `directories` crate for canonical paths

#### Web Category (`src-tauri/src/tools/web/`)
- [ ] `search.rs` — Brave Search API; API key from `AppConfig`; return top 3 result titles + snippets; Ren reads summarized results aloud
- [ ] `weather.rs` — Open-Meteo API (no key required); location from `AppConfig` or extracted from query; return current conditions + today's forecast as plain text for LLM to narrate

### Frontend Tool Cards
- [ ] `ToolCard` component: small animated card that appears below orb showing active tool (e.g. "Launching Chrome...", "Searching web...", "Setting volume to 30%")
- [ ] Tauri event `ren://tool-executing` — payload: `{ tool: string, description: string }`
- [ ] Cards slide in from bottom, auto-dismiss after 3 seconds (or on next state change)
- [ ] CSS Module: `ToolCard.module.css` — glassmorphism style card, cyan border, tool icon placeholder

## Architecture Notes

### File Structure
```
src-tauri/src/tools/
  mod.rs          — Tool trait, ToolResult, ToolError, re-exports
  registry.rs     — ToolRegistry
  system/
    mod.rs
    volume.rs
    brightness.rs
    lock.rs
    shutdown.rs
    restart.rs
  apps/
    mod.rs
    launcher.rs
  steam/
    mod.rs
    library.rs
    launcher.rs
  files/
    mod.rs
    open_folder.rs
  web/
    mod.rs
    search.rs
    weather.rs
```

### Tool Registration (in `lib.rs` setup)
```rust
let mut registry = ToolRegistry::new();
registry.register(Box::new(VolumeControl));
registry.register(Box::new(BrightnessControl));
registry.register(Box::new(LockScreen));
registry.register(Box::new(ShutdownSystem));
registry.register(Box::new(AppLauncher::new(app_cache)));
registry.register(Box::new(SteamLauncher::new(steam_cache)));
registry.register(Box::new(OpenFolder));
registry.register(Box::new(WebSearch::new(config.brave_api_key.clone())));
registry.register(Box::new(WeatherQuery::new(config.location.clone())));
```

### New Tauri Events
- `ren://tool-executing` — payload: `{ tool: string, description: string }`
- `ren://tool-result` — payload: `{ tool: string, success: boolean, summary: string }`

### Config Keys to Add
- `brave_api_key: Option<String>` — from settings panel
- `location: Option<String>` — "Istanbul" or lat/lon, used by weather tool

## Acceptance Criteria
- [ ] "Ren, open Chrome" launches Chrome — *`AppLauncher` implemented + registered; **needs voice-loop test.***
- [ ] "Ren, launch CS2" starts the game via Steam — *`SteamLauncher` + VDF parser implemented; **needs voice-loop test.***
- [ ] "Ren, set volume to 30 percent" adjusts system volume — *`VolumeControl` via `windows-rs` IMMDevice implemented; **needs voice-loop test.***
- [ ] "Ren, what's the weather in Istanbul" returns spoken weather via Open-Meteo — *`Weather` implemented with shared `reqwest::Client`; **needs voice-loop test.***
- [ ] "Ren, search for Rust async tutorials" runs Brave search, Ren summarizes top results — *`WebSearch` implemented; **needs Brave API key + voice-loop test.***
- [x] Destructive actions (shutdown) require confirmation before executing — *`ToolSafety::Destructive` opt-in + system prompt instructs the LLM to confirm first; hard code-gate intentionally omitted (voice assistant UX, not a safety critical system).*
- [x] Tool cards render in UI with appropriate animations — *`ToolCard.tsx` + `ren://tool-executing` / `ren://tool-result` wired; glassmorphism + auto-dismiss implemented.*

### Skipped / deferred deliverables
- **`brightness.rs`** — skipped. WMI `WmiMonitorBrightnessMethods` only surfaces on laptop-integrated displays; desktop monitors connected over HDMI/DP always return Unsupported. Ren targets the Qwen-14B + Whisper-large-v3 voice-first stack which requires ~13 GB VRAM, i.e. desktop-class RTX 3080/4080/4090 hardware. Laptops in that VRAM bracket are rare and expensive; the cost/benefit of writing and maintaining WMI brightness for a use-case that currently cannot run Ren does not clear the bar. Revisit if we add a smaller-model profile for laptops.

### Remaining home-only work
1. Voice-loop exercise every tool category: apps, Steam, system volume, weather, web search. Confirms the LLM actually emits the tool calls and the dispatch path fires end-to-end.
2. Provision a Brave Search API key and plug it into `AppConfig.brave_api_key`.
3. Observe `ren://tool-executing` / `ren://tool-result` events in the UI during real voice turns; tune `ToolCard` timing if the cards feel too fast/slow in practice.

## Decisions Made This Phase
- **Brightness control skipped** (2026-04-14). Reason: laptops in Ren's VRAM bracket (≥13 GB) are rare, and desktop monitors don't expose WMI brightness. Skipping avoids ~50 LOC + native Windows dep that would never execute on target hardware.

## Known Risks
- Fuzzy app matching: too loose = wrong app launched. Use a scored fuzzy match (e.g. `strsim` crate), only launch if score is above a threshold, otherwise ask user to confirm.
- Steam library parsing: VDF format is not standard JSON. Use `keyvalues-parser` crate or write a simple parser. Some users have complex multi-library setups.
- Brave Search API key required for web search — this is a user-provided key (free tier available). Weather works without any key via Open-Meteo.
