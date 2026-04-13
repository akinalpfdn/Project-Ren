# Ren — Development Plan

## Overview

Ren is a fully local, voice-first personal assistant for Windows. It runs entirely on the user's machine — no cloud inference, no API fees, no data leaves the PC. Users wake Ren by voice, speak commands in Turkish, and Ren responds in English with a calm, dry, JARVIS-inspired personality. Ren can launch applications and Steam games, control system settings, search the web, manage calendar and weather, control media playback, and more. The long-term vision is a shippable single-executable product where the end user downloads one file, double-clicks, and Ren takes care of the rest.

Ren is designed for users who want a personal AI assistant that respects privacy, has no recurring costs, and feels alive — an assistant that is "always there" but invisible until called.

## Platform & Stack

- **Target**: Windows 11 (x64, with NVIDIA GPU). Future ports to macOS possible but out of scope.
- **Framework**: Tauri v2 (Rust backend + React frontend)
- **Backend language**: Rust
- **Frontend language**: TypeScript + React
- **Styling**: Tailwind CSS + Framer Motion (animations)
- **Architecture**: Layered, event-driven. Rust core handles all audio, inference, and OS integration. React frontend handles visualization and settings UI. Communication via Tauri IPC events.
- **Design pattern notes**: State machine pattern for Ren's lifecycle (sleeping / waking / listening / thinking / speaking / idle). Strategy pattern for tool executors (each capability implements a common executor interface). Observer pattern for UI state updates via Tauri event system.
- **Localization**: Prepared for future. Initial release: UI in English only, voice input in Turkish, voice output in English. All user-facing strings in the React frontend must go through an i18n layer (react-i18next) from day one. All Rust-side log messages and error messages should be keyed strings, not hardcoded literals.
- **Theming**: Dark only (futuristic aesthetic — deep blacks, cyan glows, subtle gradients). All colors, typography, spacing, glow intensities, animation timings must live in a central theme file. No hardcoded colors anywhere. The theme file is the single source of truth — if the aesthetic is tuned later, one file changes.

### Key dependencies (Rust backend)

- `tauri` — application shell
- `cpal` — cross-platform audio I/O (microphone capture, speaker playback)
- `webrtc-vad` or `voice_activity_detector` — voice activity detection
- `porcupine` — wake word detection with custom "Ren" model
- `whisper-rs` — whisper.cpp bindings for local STT (Turkish)
- `ort` — ONNX Runtime for Kokoro TTS model
- `rodio` — audio playback for TTS output and UI notification sounds
- `reqwest` — HTTP client for web APIs (Brave Search, Google Calendar, Open-Meteo, Spotify)
- `tokio` — async runtime
- `serde` / `serde_json` — serialization
- `rspotify` — Spotify Web API client
- `global-hotkey` — system-wide hotkey registration
- `tray-icon` — Windows system tray integration
- `windows` (windows-rs) — Windows native APIs for system control
- `directories` — cross-platform user data directory resolution

### Key dependencies (React frontend)

- `react` — UI framework
- `@tauri-apps/api` — Tauri IPC bindings
- `framer-motion` — animation library
- `tailwindcss` — utility-first styling
- `react-i18next` — localization layer
- `zustand` — lightweight state management

## Constraints & Platform Considerations

### Windows-specific

- **Microphone permission**: Windows 11 requires explicit microphone permission. Ren must detect permission denial gracefully and guide the user to Settings > Privacy > Microphone.
- **Always-on-top window behavior**: Ren's main orb window uses Tauri's `alwaysOnTop: true` plus `skipTaskbar: true`. Window should not appear in Alt+Tab.
- **Autostart on boot**: Optional feature, must be explicit opt-in via settings. Use Windows Registry `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` for this.
- **NVIDIA driver dependency**: Ollama (used in portable mode as a child process — see Architecture) requires a recent NVIDIA driver for CUDA acceleration. Ren must detect driver absence at startup and warn the user. CPU-only inference for a 14B model is unacceptably slow.
- **Ollama port management**: Ren launches its own Ollama child process bound to a custom port (default 11500, falling back to next available if occupied). This avoids collision with any pre-existing system Ollama installation on port 11434. Port selection logic must be robust — try preferred port, then probe for free port, then store the chosen port in config for the session.
- **Windows Media Controls integration**: Use Windows Runtime (WinRT) `GlobalSystemMediaTransportControlsSessionManager` for universal media play/pause/next/prev across all media apps. Accessible via `windows-rs` crate.
- **Startup assets directory**: All downloaded models and runtime data must live in `%APPDATA%\Ren\` — never in Program Files, never require admin rights.
- **No installer, no admin for Ren itself**: Ren ships as a portable single executable. No MSI, no NSIS. Ren itself does not require admin elevation — all its files live under `%APPDATA%`. Some tool actions (system shutdown, certain Windows controls) may legitimately require elevation when invoked, and that is acceptable on a per-action basis with proper UAC prompts. The principle is: launching Ren never asks for admin, but specific actions Ren takes might.
- **Code signing**: For end-user trust and SmartScreen reputation, the final binary should be signed with a code signing certificate. Note this as a deployment step, not a development blocker.
- **Antivirus false positives**: Binaries that embed llama.cpp and bundle large model files may trigger heuristic AV warnings. Test against Windows Defender early.

### Hardware assumptions

- Minimum target: NVIDIA GPU with 12GB VRAM (RTX 4070 Ti class). Below this, Qwen 14B does not fit entirely in VRAM and performance drops sharply.
- Minimum RAM: 16GB. Recommended: 32GB+.
- Disk: ~15GB free for models and runtime data.
- Ren must check these at first launch and warn the user if the system is below minimum spec.

### Privacy & network

- Ren is local-first by design. The only network calls are: (1) optional first-run model download, (2) web search API calls when the user asks, (3) calendar/weather/Spotify API calls when the user asks. No telemetry. No crash reporting by default. If crash reporting is added later, it must be opt-in.
- No conversation data ever leaves the machine. STT, LLM, and TTS all run locally.
- All API credentials (Spotify client ID, Google OAuth client ID, Brave API key) are baked into the binary but stored in a way that does not expose secrets. For Spotify specifically, use OAuth PKCE flow so no client secret is needed on the client side.

## Architecture

### High-level layout

```
+-----------------------------------------------+
|              Tauri Application                |
|                                               |
|  +--------------------+  +-----------------+  |
|  |   Rust Core        |  |  React Frontend |  |
|  |                    |  |                 |  |
|  |  Audio Pipeline    |  |  Orb View       |  |
|  |  State Machine     |<>|  Transcript     |  |
|  |  LLM Engine        |  |  Tool Cards     |  |
|  |  STT Engine        |  |  Settings Panel |  |
|  |  TTS Engine        |  |                 |  |
|  |  Wake Word         |  |                 |  |
|  |  Tool Executors    |  |                 |  |
|  |  Config & Storage  |  |                 |  |
|  +--------------------+  +-----------------+  |
|           ^                      ^            |
|           |                      |            |
|           +------ IPC Events ----+            |
+-----------------------------------------------+
```

### State machine

Ren's core is a state machine with these states:

- **Initializing** — app just launched, loading models, running first-run setup if needed
- **Sleeping** — only wake word detector active, everything else unloaded or idle
- **Waking** — wake word triggered, playing acknowledgment sound, warming up STT and LLM in parallel, showing "listening" visual
- **Listening** — VAD active, capturing user audio, waiting for end-of-speech
- **Thinking** — audio captured, running STT → LLM → tool calls → TTS synthesis
- **Speaking** — playing TTS audio through speakers, showing reactive waveform
- **Idle** — finished speaking, waiting for follow-up (user can speak without wake word for N seconds)
- **Error** — something failed, show error state, return to Sleeping after user acknowledgment

Each state transition emits a Tauri event. The React frontend listens to these events and animates accordingly. State transitions are strictly one-way through a central state manager — no component can mutate state directly, only request transitions.

### LLM engine subsystem (Ollama portable child)

Ren does not require the user to install Ollama. Instead, Ren manages its own private Ollama instance as a child process:

- **Binary location**: `%APPDATA%\Ren\bin\ollama.exe`. Downloaded on first run from the official Ollama GitHub releases. Just the binary, never the system-wide installer.
- **Model storage**: `%APPDATA%\Ren\models\ollama\` (overridden via `OLLAMA_MODELS` environment variable when launching the child). This isolates Ren's models from any system-wide Ollama the user may have installed for other purposes.
- **Process lifecycle**: When Ren starts, it spawns `ollama serve` as a managed child process with stdout/stderr piped back to Ren's logs. When Ren exits (gracefully or via crash), the child process is terminated. Ren is the parent and owns the child's full lifecycle — no service, no autostart on boot, no orphaned processes.
- **Port selection**: Ren's Ollama child binds to a non-default port (preferred: 11500). If the preferred port is occupied, Ren probes for the next free port in a defined range and uses that. The selected port is stored in the runtime session config and used for all HTTP calls during the session. This guarantees no collision with a system-wide Ollama (which uses 11434 by default) or with other applications.
- **Health checks**: Ren probes the child Ollama's `/api/tags` endpoint after launch to confirm readiness before transitioning out of Initializing state. Timeouts and retries handle slow startup gracefully.
- **Communication**: Ren's Rust core talks to its child Ollama via standard HTTP (`reqwest`) on `localhost:<port>`. Streaming responses for low-latency token-by-token output to the TTS pipeline.
- **Model management**: On first run, Ren issues `ollama pull qwen2.5:14b` against its own child instance (or, equivalently, downloads the GGUF directly from Hugging Face and places it in the model directory using the Ollama manifest format). Progress is surfaced to the UI for the futuristic download animation.
- **Pre-existing Ollama detection**: If the user already has a system-wide Ollama installed, Ren ignores it entirely and runs its own. The two are completely independent.

This approach keeps Ren self-contained while leveraging Ollama's mature GPU detection, CUDA setup, model lifecycle, and streaming API. From the user's perspective, there is "Ren" and only Ren — they never need to know Ollama exists.

### Audio pipeline

Audio flows through these stages in order:

1. **Microphone capture** (cpal) — continuous 16kHz mono stream in Sleeping state, dedicated capture buffer in Listening state.
2. **Wake word detector** (Porcupine) — consumes the continuous stream in Sleeping state, emits a wake event when "Ren" is detected.
3. **Voice activity detector** — consumes audio in Listening state, detects end-of-speech (~500ms of silence).
4. **STT engine** (whisper-rs) — runs on captured audio buffer, produces Turkish transcript.
5. **LLM engine** (Ren's child Ollama via HTTP) — receives transcript plus system prompt plus tool schemas, produces English response text and/or tool calls. Streamed token-by-token.
6. **Tool execution** — if LLM emits tool calls, route them to appropriate executors, collect results, feed back to LLM for final response.
7. **TTS engine** (Kokoro ONNX) — receives final English response, produces audio buffer.
8. **Speaker playback** (rodio) — plays TTS audio, sends playback progress to frontend for waveform visualization.

All stages run in dedicated tokio tasks. Communication between stages uses channels. The state machine orchestrates which stages are active based on the current state.

### Tool system

Tools are the capabilities Ren exposes to the LLM. Each tool implements a common trait with:

- A **schema** (JSON) that the LLM sees and uses to format function calls
- An **executor** function that takes parsed parameters and returns a result
- A **description** used by the LLM to decide when to call the tool

Tools are registered at startup into a central `ToolRegistry`. The registry builds the system prompt's tool list and dispatches LLM tool calls to the right executor.

Tools are grouped into categories for organization: `system`, `apps`, `media`, `web`, `calendar`, `weather`, `files`, `meta`. Each category lives in its own module.

### First-run setup flow

1. App launches, detects missing models or missing Ollama binary in `%APPDATA%\Ren\`.
2. Shows a welcome screen with futuristic animation.
3. Hardware check: detect NVIDIA GPU and driver version, available VRAM, free disk space. If sub-spec, show a warning before proceeding.
4. Downloads required runtime components in sequence with progress UI:
   - Ollama portable binary (~150MB) → `%APPDATA%\Ren\bin\ollama.exe`
   - Whisper large-v3 ggml (~3GB) → `%APPDATA%\Ren\models\whisper\` — STT
   - Kokoro ONNX model (~300MB) — TTS (may be bundled in binary if size permits)
5. Verifies SHA256 hashes of all downloaded files.
6. Spawns Ollama child process on a private port.
7. Pulls Qwen 2.5 14B Q4_K_M (~9GB) via the child Ollama instance, surfacing pull progress to the UI.
8. Loads Whisper and Kokoro models into memory.
9. Plays introduction: Ren speaks a welcome message in its own voice.
10. Transitions to Sleeping state, shows the idle orb.

If any download is interrupted, Ren resumes from where it left off on next launch. Each download stage is independent and idempotent.

### Lazy-eager model loading

To minimize latency while respecting resource constraints:

- **Wake word model**: Always loaded (minimal footprint).
- **Whisper model**: Unloaded when idle. On wake word trigger, loading begins immediately in parallel with the acknowledgment sound playback. By the time the user finishes speaking, Whisper is ready.
- **LLM model**: Managed via Ollama's `keep_alive` parameter on each request (default: 30 minutes). On wake word trigger, if the model is not loaded, Ren issues a small no-op generation to warm it in parallel with STT. Ollama handles GPU memory directly.
- **TTS model**: Lightweight (~300MB), kept loaded after first use.

This eager loading on wake word trigger is critical — it hides model load latency behind the user's own speaking time.

### Storage layout

```
%APPDATA%\Ren\
  config.json              - User settings (volume, wake sensitivity, API tokens, Ollama port)
  credentials.db           - Encrypted storage for OAuth tokens (Spotify, Google)
  bin\
    ollama.exe             - Ren's private Ollama binary
  models\
    ollama\                - Ollama model storage (OLLAMA_MODELS env override)
      blobs\
      manifests\
    whisper\
      ggml-large-v3.bin
    kokoro\
      kokoro.onnx
    porcupine\
      ren.ppn
  logs\
    ren.log                - Ren application log (rotating)
    ollama.log             - Stdout/stderr from child Ollama process
  cache\
    spotify_library.json   - Cached user library
    steam_games.json       - Cached Steam library
```

## Feature Scope

### MVP (Phases 1–5)

- Wake word detection with custom "Ren" model
- Turkish STT via local Whisper
- LLM reasoning and response generation via local Qwen 2.5 14B
- English TTS via local Kokoro
- State machine (sleeping → waking → listening → thinking → speaking → idle)
- Sleep/wake by voice and hotkey
- Conversation mode (follow-up without re-triggering wake word)
- Core futuristic orb UI with idle/listening/thinking/speaking animations
- System tray integration with basic controls
- Application launcher (by name, matched against installed apps)
- Steam game launcher (via Steam URI protocol)
- System control tools (volume, lock, shutdown with confirmation, brightness)
- Folder opening (Downloads, Documents, Desktop, etc.)
- Web search via Brave Search API
- Weather via Open-Meteo (no API key required)
- Google Calendar integration (read today's events, upcoming events)
- Media control via Windows Media Transport Controls (play/pause/next/prev/volume)
- Spotify control via Web API (search, play, playlists) — requires user's Premium account
- Settings panel for microphone, sensitivity, autostart, API connections

### Post-MVP

- File search integration (Everything or Windows Search)
- Clipboard read/write tools ("translate what's in my clipboard")
- Screenshot + OCR tools
- Reminder/timer system with voice notifications
- Additional launcher support (Epic Games, GOG, Battle.net)
- Custom wake word training UI (change from "Ren" to anything)
- Voice cloning for personalized Ren voice
- Vision model integration for screen understanding
- Developer tooling category (git status, project switching, Claude Code integration)
- Windows notification forwarding to Ren
- Multi-language UI localization (Turkish, German, etc.)
- Auto-updater
- Crash reporting (opt-in)
- macOS port

## Phases

### Phase 1: Tauri Shell and Core Orb UI

**Goal**: The app launches, shows a futuristic orb in a small always-on-top window, and animates between mock states.

**Delivers**:
- Tauri v2 project scaffolded with React + TypeScript + Tailwind + Framer Motion
- Small, frameless, always-on-top, transparent-background window positioned at screen bottom-right
- Central orb component with smooth "breathing" idle animation
- Mock state machine in React with buttons (for now) to trigger state transitions: idle → listening → thinking → speaking → idle
- Each state has a distinct visual: idle is breathing glow, listening is reactive pulse, thinking has orbital particles, speaking has waveform
- Theme file with all design tokens (colors, glows, timings, spacing)
- i18n scaffolding with English strings
- System tray icon with "Show/Hide Ren" and "Quit" menu items

**Depends on**: Nothing

**Acceptance criteria**:
- [ ] Ren.exe launches a window with animated orb
- [ ] Window is always on top, does not appear in taskbar or Alt+Tab
- [ ] Tray icon present, menu works
- [ ] Clicking tray icon toggles window visibility
- [ ] Mock state transitions animate smoothly
- [ ] No hardcoded colors or strings — all via theme and i18n layers

### Phase 2: Audio Pipeline Foundation

**Goal**: Ren can capture audio from the microphone, detect speech, transcribe it locally with Whisper, and display the transcript on screen. No LLM yet.

**Delivers**:
- Rust-side audio capture via cpal (16kHz mono)
- Voice activity detection integrated
- Whisper-rs integration with large-v3 model
- Model is downloaded on first run if absent, stored in `%APPDATA%\Ren\models\`
- Hash verification after download
- First-run download UI with progress bar and futuristic "initializing" animation
- Push-to-talk hotkey (Ctrl+Alt+R by default): hold to record, release to transcribe
- Transcript displayed in the frontend below the orb
- State machine transitions from idle → listening → thinking (STT running) → idle
- Turkish transcription confirmed working

**Depends on**: Phase 1

**Acceptance criteria**:
- [ ] First launch shows download screen, downloads Whisper model with progress
- [ ] Holding Ctrl+Alt+R captures audio, releasing transcribes it
- [ ] Turkish speech is accurately transcribed
- [ ] Transcript appears in UI below orb
- [ ] State transitions correctly reflect audio pipeline activity
- [ ] Model load happens lazily on first push-to-talk, not at startup

### Phase 3: Local LLM via Portable Ollama, and TTS

**Goal**: Ren understands transcribed speech, generates English responses via a private Ollama child process running Qwen 2.5 14B, and speaks them out loud via Kokoro TTS.

**Delivers**:
- Ollama portable binary download manager: detects missing binary, downloads from GitHub releases, verifies hash, places in `%APPDATA%\Ren\bin\`
- Ollama child process supervisor in Rust: spawns `ollama serve` with `OLLAMA_MODELS` set to Ren's private model directory and `OLLAMA_HOST` set to a non-default port
- Port selection logic: try preferred port (11500), probe for free port if occupied, store selected port in session config
- Health check loop after spawn: poll `/api/tags` until ready or timeout
- Process lifecycle: child terminates cleanly when Ren exits, including on crash via process-group kill on Windows
- Qwen 2.5 14B Q4_K_M model pull on first run via the child Ollama instance, with progress streamed to UI (resumable)
- HTTP client wrapper around Ollama's `/api/chat` with streaming support
- System prompt defining Ren's personality (JARVIS-inspired: calm, dry, concise, addresses user as "sir" occasionally), language rules (Turkish in, English out), and exception for when user explicitly asks for content in another language
- Kokoro TTS integration via ort (ONNX Runtime), model bundled in binary or downloaded on first run
- Streaming pipeline: as Ollama emits tokens, sentence-level chunks are sent to TTS to minimize time-to-first-audio
- Full pipeline end-to-end: push-to-talk → STT → LLM (via private Ollama) → TTS → speaker
- Speaking state animates waveform from actual TTS audio output
- Conversation history maintained per session (in memory)
- GPU detection at startup; warning if no NVIDIA GPU
- Detection of any pre-existing system Ollama: documented as known and ignored — Ren uses its own instance regardless

**Depends on**: Phase 2

**Acceptance criteria**:
- [ ] Ren downloads Ollama binary and Qwen 14B on first launch with progress UI
- [ ] Ollama child process starts on a custom port and survives until Ren exits
- [ ] Ren responds to Turkish speech with English speech
- [ ] Personality is consistent across turns
- [ ] Time-to-first-audio under 2 seconds on target hardware after warm-up
- [ ] Conversation history works for follow-up questions within a session
- [ ] Killing Ren cleanly terminates the child Ollama process — no orphans in Task Manager
- [ ] Pre-existing system Ollama (if user has one) does not interfere with Ren's instance
- [ ] Downloads are resumable if interrupted

### Phase 4: Wake Word and Full Conversation Loop

**Goal**: Ren is summoned by voice. User says "Ren", Ren wakes up, conversation proceeds naturally, Ren returns to sleep after inactivity or explicit dismissal.

**Delivers**:
- Porcupine integration via its Rust bindings
- Two custom wake word models produced via the Picovoice Console (free tier) by the developer: `hey_ren_en_windows.ppn` and `ren_uyan_en_windows.ppn` — both bundled as Tauri resources inside the binary (~50KB each)
- Picovoice AccessKey for free tier embedded in Ren (this is standard practice for Porcupine free tier — the key authenticates Ren as a registered personal-use application)
- Porcupine initialized with both keywords simultaneously; triggering either wakes Ren
- Sleeping state: only wake word detector active, all heavy models unloaded from VRAM
- Wake word detected → Waking state → acknowledgment sound → parallel model loading → Listening state
- VAD-driven end-of-speech detection (no push-to-talk needed when awake)
- Conversation mode: after Ren responds, remains in Idle state for 30 seconds awaiting follow-up without wake word
- Idle timeout returns to Sleeping and unloads models
- Voice dismissal: phrases like "thanks that's all", "goodbye Ren", "sleep" return to Sleeping immediately
- Hotkey still works as override for push-to-talk and for force-sleep
- Visual state indicators refined (sleeping is very subtle, waking is dramatic, listening is attentive)

**Depends on**: Phase 3

**Acceptance criteria**:
- [ ] Saying "Ren" from across the room reliably wakes the assistant
- [ ] Models load in parallel with wake animation so there's no noticeable delay
- [ ] Follow-up questions work without re-triggering wake word
- [ ] Idle timeout returns to sleep after configurable duration
- [ ] Voice dismissal works
- [ ] False positive rate on wake word is acceptable (not triggering on background conversation)

### Phase 5: Tool System and First Tool Categories

**Goal**: Ren can take real actions on the system. Application launching, Steam games, system controls, folder opening, and web search all work via LLM tool calls.

**Delivers**:
- `ToolRegistry` architecture with trait-based executor pattern
- LLM prompt updated to include tool schemas
- Tool call parsing from LLM output (JSON mode or structured output via llama.cpp grammar)
- Tool execution pipeline: LLM emits tool call → execute → feed result back to LLM → final response to user
- **System category**: volume control, screen lock, shutdown/restart with 10-second voice-cancelable confirmation, brightness (if supported)
- **Apps category**: application launcher using cached Start Menu apps list; fuzzy matching; handles common aliases
- **Steam category**: parses local Steam library (libraryfolders.vdf + appmanifest files), caches on first run, launches via steam:// URI
- **Files category**: open common folders (Downloads, Documents, Desktop, user-defined favorites)
- **Web category**: Brave Search API integration; Ren reads summarized results aloud
- Tool execution cards in the frontend: small animated cards showing what Ren is doing (launching, searching, etc.)
- Confirmation flow for destructive actions (shutdown, close-all-apps)

**Depends on**: Phase 4

**Acceptance criteria**:
- [ ] "Ren, open Chrome" launches Chrome
- [ ] "Ren, launch CS2" starts the game via Steam
- [ ] "Ren, set volume to 30 percent" adjusts system volume
- [ ] "Ren, what's the weather in Istanbul" returns spoken weather via Open-Meteo
- [ ] "Ren, search for Rust async tutorials" runs Brave search, Ren summarizes top results
- [ ] Destructive actions require confirmation
- [ ] Tool cards render in UI with appropriate animations

### Phase 6: Calendar, Weather, Media, Spotify

**Goal**: Ren becomes truly useful as a daily assistant with context awareness and media control.

**Delivers**:
- **Weather category**: Open-Meteo integration (no API key); current conditions and forecasts; handles location via user config or spoken query
- **Calendar category**: Google Calendar OAuth flow via UI settings panel; read-only access to today's and upcoming events; Ren can answer "what's on my calendar"
- **Media (universal)**: Windows Media Transport Controls integration via windows-rs — play, pause, next, previous, volume work against whatever media app is active (Spotify, YouTube in browser, VLC, etc.)
- **Spotify category**: rspotify OAuth PKCE flow via UI settings panel; user logs in with their own Premium account once; Ren can search and play specific songs, albums, playlists, artists; fallback gracefully to Media Transport Controls if user is not Premium or not logged in
- Settings panel in UI: "Connect Spotify", "Connect Google Calendar" buttons; status indicators; disconnect option
- OAuth tokens stored encrypted in `%APPDATA%\Ren\credentials.db`

**Depends on**: Phase 5

**Acceptance criteria**:
- [ ] "Ren, what's on my calendar today" reads events via Google Calendar
- [ ] "Ren, pause the music" works regardless of which media app is playing
- [ ] "Ren, play Radiohead on Spotify" starts playback for Premium users
- [ ] OAuth flows complete entirely within Ren UI — no command line, no manual token pasting
- [ ] Credentials survive Ren restart
- [ ] Disconnect/reconnect works cleanly

### Phase 7: Single-Executable Packaging and First-Run Polish

**Goal**: Ren ships as a single portable Windows executable. End user downloads Ren.exe, double-clicks, experiences a polished first-run setup, and is talking to Ren within minutes.

**Delivers**:
- Tauri build configuration tuned for single-exe output
- whisper.cpp and Kokoro ONNX runtime statically linked into the Rust binary where possible
- Porcupine model and any small static assets bundled as Tauri resources
- Ollama binary downloaded on first run (not bundled — keeps Ren.exe small and avoids redistribution license concerns)
- Large models (Qwen via Ollama pull, Whisper) downloaded on first run
- First-run wizard: welcome screen → hardware check (GPU, RAM, disk) → sequential downloads with animated progress (Ollama binary → Whisper model → Kokoro model → Qwen via Ollama pull) → voice introduction ("Hello sir, I am Ren...") → ready
- Hardware check explains clearly if user is below spec (no CUDA GPU, insufficient VRAM, insufficient disk)
- All data stored in `%APPDATA%\Ren\` — no registry writes except optional autostart
- **Uninstall flow**: Settings panel includes an "Uninstall Ren" action. On confirmation, Ren terminates its child Ollama process, deletes `%APPDATA%\Ren\` entirely, and self-deletes its own executable via a small batch script trick. After this, no trace of Ren remains on the system.
- **Smart uninstall safety**: Before deletion, Ren explicitly informs the user that this will remove its private Ollama instance located inside `%APPDATA%\Ren\`, and confirms that any system-wide Ollama installation (if present) will not be affected. This single confirmation step is the only one — keeps the experience close to "one click" while preventing surprises for users who run multiple AI tools.
- Code signing integrated into build pipeline (note: certificate procurement is a separate task)
- README.md, LICENSE, .gitignore appropriate to a Rust/Tauri project
- Final binary is tested against Windows Defender for false positives

**Depends on**: Phase 6

**Acceptance criteria**:
- [ ] Downloading and running a single Ren.exe is the entire installation process
- [ ] Ren itself never requests admin privileges to launch
- [ ] First-run wizard completes successfully on a clean Windows 11 machine
- [ ] First-run wizard completes successfully on a Windows 11 machine that already has Ollama installed system-wide (no conflict)
- [ ] Hardware check gracefully handles sub-spec machines
- [ ] Ren fully operational within reasonable time on a typical home connection
- [ ] Uninstall removes Ren completely with one user confirmation, leaving no traces outside any pre-existing system Ollama
- [ ] Windows Defender does not flag the binary

## Implementation Guidelines

These apply to ALL phases. Claude Code must follow these throughout development.

- Follow SOLID principles — especially Single Responsibility and Dependency Inversion. Tool executors, audio stages, and state handlers must be independently testable and replaceable.
- Use the Strategy pattern for tool executors. Each tool implements a common trait. Adding a new tool should require zero changes to the registry or dispatcher.
- Use the State pattern for Ren's lifecycle. State transitions are explicit and traceable through logs. No component mutates state directly — all changes go through the central state manager.
- Use the Observer pattern for UI updates. Rust core emits state events, React listens. Never poll from the frontend.
- Write clean, self-documenting code. Comments explain "why", never "what". Rust type signatures and function names carry the intent.
- **Theming is sacred**. All colors, typography, spacing, animation durations, glow intensities live in a single theme file consumed via Tailwind config and React components. No hardcoded colors or magic numbers in view code.
- **Localization-ready from day one**. All user-facing strings in the React frontend go through react-i18next, even though only English ships initially. Rust-side messages that might be surfaced to the UI are returned as i18n keys, not translated strings.
- Error handling must be explicit and meaningful. No silent catches. No "unknown error" messages. Every failure mode the user can encounter has a specific, actionable message.
- No hardcoded values anywhere. Ports, paths, keep-alive durations, audio sample rates, wake sensitivity — all in a central config module with sensible defaults.
- The central config file (`%APPDATA%\Ren\config.json`) is the single source of truth for runtime settings. UI changes write to it. Rust reads from it. Hot reload on file change is nice-to-have but not required.
- Tool schemas (the JSON exposed to the LLM) must be generated from the same Rust type definitions that implement the executor — use serde and schema derivation to avoid drift between schema and implementation.
- All API integrations (Brave, Google Calendar, Spotify, Open-Meteo) must be isolated behind a trait so they can be swapped or mocked. No API client code leaks into tool executors directly.
- Logs go to `%APPDATA%\Ren\logs\ren.log` with rotation. `tracing` crate for structured logging.
- The repository is public/open source. Generate a proper README.md, LICENSE (MIT or Apache 2.0 — confirm with user), .gitignore for Rust + Node, and CONTRIBUTING.md explaining the tool system architecture for contributors who want to add new capabilities.
- Respect existing code patterns when modifying existing code.

## Risks & Watch Items

- **Ollama child process lifecycle on Windows**: Cleanly killing a child process tree on Windows is non-trivial. If Ren crashes without graceful shutdown, the child Ollama may be orphaned. Use a Windows Job Object to bind child to parent so OS kills child if parent dies. Test crash scenarios explicitly.
- **Port collision**: Even with non-default port (11500), some other application could occupy it. Port probing logic must be robust and the chosen port must be communicated reliably between Ren startup and the HTTP client. Probe a range, fail loudly if no port is free.
- **Pre-existing system Ollama detection**: While Ren ignores any system Ollama and runs its own, the user might be confused if they see two Ollama processes in Task Manager. Document this clearly in the README and in any user-facing diagnostics.
- **Ollama binary version drift**: Ollama updates frequently. Ren should pin a known-good version of the Ollama binary it downloads, not always pull "latest". Provide a mechanism to update the bundled Ollama version through Ren updates.
- **Binary size and Windows Defender**: A 300+ MB Rust binary embedding ML runtimes plus a downloaded Ollama executable is unusual and may trigger heuristic AV scanning. Test against Defender continuously, not just at the end.
- **Porcupine licensing**: Picovoice Porcupine's free tier is generous for personal use but has restrictions on commercial redistribution. If Ren is distributed at scale, licensing terms must be revisited. Have openWakeWord as a fallback plan in mind.
- **Kokoro Turkish output**: Kokoro is English-optimized. If the user later wants Turkish TTS output, Kokoro will not suffice and a different TTS (Piper, XTTS-v2) must be integrated. The TTS layer must be abstracted behind a trait to allow this swap later.
- **Wake word false positive rate**: Custom wake words trained on short names ("Ren") are prone to false positives on similar-sounding words in background conversation. Test extensively in realistic noisy environments before committing to the default wake word. Consider "Hey Ren" as an alternative if "Ren" alone proves unreliable.
- **Whisper Turkish accuracy on short phrases**: Whisper large-v3 is excellent on Turkish in general but can struggle with very short utterances or proper nouns. Consider prompt biasing with common Turkish command verbs.
- **VRAM pressure during active use**: Qwen 14B + Whisper large-v3 together push 12GB of VRAM to its limits. Monitor real VRAM usage during end-to-end conversations and consider falling back to Whisper medium if memory pressure causes OOM on RTX 4070 Ti.
- **Model download sources**: Ren does not host any models. All downloads come from existing public infrastructure:
  - Qwen 2.5 14B via `ollama pull` against Ollama's official model registry
  - Whisper large-v3 ggml from the `ggerganov/whisper.cpp` GitHub releases (or Hugging Face mirror)
  - Kokoro ONNX from Hugging Face (`onnx-community/Kokoro-82M-v1.0-ONNX`)
  - Ollama binary from `github.com/ollama/ollama/releases`
  - Porcupine wake word `.ppn` files bundled inside Ren.exe itself (not downloaded)
  No CDN costs for the Ren project — bandwidth is provided by Ollama, Hugging Face, and GitHub. Monitor for upstream changes (URL paths, breaking changes in file formats) but there is no scaling risk from download bandwidth.
- **Spotify rate limits**: Spotify Web API has rate limits per app (not per user). At scale, all users sharing one Spotify app ID could hit shared limits. Monitor this and be prepared to register additional app IDs or rotate.
- **First-run UX on slow connections**: 12GB total download on a 10 Mbps connection takes hours. The first-run wizard must be robust, resumable, and reassuring. Users should be able to close and reopen Ren mid-download.
- **Open questions to resolve during development**:
  - Wake words: default is both "Hey Ren" and "Ren uyan" enabled simultaneously (Porcupine supports multi-keyword detection). User can disable either in settings. Test false positive rates for both during Phase 4 and adjust sensitivity per keyword.
  - Default voice: `bf_emma` (British Female, calm and elegant — natural fit for JARVIS-inspired personality) as primary default, with `af_bella` (American Female, warm) as alternative. Test both with actual Ren responses during Phase 3 and pick whichever feels right. Voice blending (e.g. `bf_emma:0.7,af_bella:0.3`) is supported by Kokoro if a custom mix is preferred. Settings panel should expose voice selection as a dropdown.
  - Should autostart-on-boot be opt-in at first run, or only from settings
  - Which LICENSE to use (MIT vs Apache 2.0)
  - Whether to download the Qwen GGUF directly from Hugging Face or rely on `ollama pull` against the child instance — both work, the second is simpler
  - Pinned Ollama version to bundle, and update cadence
