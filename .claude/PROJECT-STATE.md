# Project Ren — Current State
Last updated: 2026-04-13 (Phase 5 start)

## Active Phase
**Phase 5** — Tool system and first tool categories

## Phase Files
| File | Status |
|------|--------|
| `.claude/phases/PHASE-01-done.md` | DONE |
| `.claude/phases/PHASE-02-done.md` | DONE (home: STT model verification) |
| `.claude/phases/PHASE-03-done.md` | DONE (home: Ollama runtime + Kokoro ORT inference) |
| `.claude/phases/PHASE-04-done.md` | DONE (home: Picovoice key + .ppn files) |
| `.claude/phases/PHASE-05-active.md` | ACTIVE ← you are here |
| `.claude/phases/PHASE-06-pending.md` | PENDING |
| `.claude/phases/PHASE-07-pending.md` | PENDING |

## What Exists in the Repo Right Now

### Rust Backend (`src-tauri/src/`)
- `config/defaults.rs` — All constants (ports, sample rates, filenames, hashes, wake keywords, dismissal phrases, Picovoice key resolved via `option_env!`)
- `config/mod.rs` — `AppConfig`, load/save, path helpers (`app_data_dir`, `models_dir`, `bin_dir`, …)
- `state/mod.rs` — `RenStateMachine`, 8 states, transition validation, Tauri event emit, `tokio::sync::broadcast` notifier with `subscribe()`
- `audio/capture.rs` — cpal 16 kHz mono mic capture → channel
- `audio/vad.rs` — VAD task, `SpeechStart` / `SpeechEnd` events, 500 ms silence threshold
- `audio/mod.rs` — `start_pipeline()` combining capture + VAD
- `stt/mod.rs` — `SttEngine` trait
- `stt/whisper.rs` — `WhisperEngine`, lazy load, gated on `stt`
- `download/verify.rs` — SHA256 verification
- `download/mod.rs` — Resumable HTTP download, `.part` files, progress events
- `hotkey/mod.rs` — Ctrl+Alt+R (PTT), Ctrl+Alt+S (force sleep)
- `commands/mod.rs` — toggle/show/hide window, `get_state` Tauri commands
- `llm/ollama_process.rs` — child process + Windows Job Object + port probing
- `llm/client.rs` — `OllamaClient` with SSE streaming
- `llm/prompt.rs` — JARVIS system prompt
- `llm/conversation.rs` — per-session history
- `llm/mod.rs` — `run_turn()` + sentence-boundary splitter
- `tts/mod.rs` — `TtsEngine` trait
- `tts/kokoro.rs` — `KokoroEngine` (gated on `tts`, ORT inference TODO for home)
- `playback/mod.rs` — `AudioPlayer` (rodio), waveform RMS computation
- `wake/mod.rs` — `WakeEngine` trait, `WakeKeyword` config, `WakeEvent` payload
- `wake/porcupine.rs` — `PorcupineWakeEngine`, gated on `wake`, native Picovoice integration
- `dismissal/mod.rs` — Pure-Rust dismissal phrase detector with unit tests
- `lib.rs` — Full wiring + state observer tasks (`spawn_conversation_timer`, `spawn_model_unloader`)

### Frontend (`src/`)
- `config/ui.ts` — Shared UI constants (`WAVEFORM_BAR_COUNT`, `TRANSCRIPT_VISIBLE_MS`)
- `utils/format.ts` — `formatBytes`, `formatSpeed`
- `types/index.ts` — `RenState` + all Tauri event payload types
- `store/index.ts` — Zustand: `currentState`, `error`, `transcript`, `downloadProgress`, `waveformAmplitudes`
- `hooks/useRenEvents.ts` — Tauri event listeners → store (selector pattern, transcript timeout via `useRef`)
- `components/Orb.{tsx,module.css}` — State-driven visuals; data-driven waveform; dramatic waking burst
- `components/Transcript.{tsx,module.css}` — STT transcript display
- `components/DownloadOverlay.{tsx,module.css}` — Download progress UI; CSS custom property for the bar fill
- `App.{tsx,module.css}` — Root composition (`Orb`, `Transcript`, `DownloadOverlay`)
- `styles/theme.css` — All design tokens (one source of truth)
- `styles/global.css` — Resets
- `i18n/index.ts` + `locales/en.json` — i18n init + English strings (states, errors, settings, welcome, download steps, debug)

### Project Files
- `DEVPLAN.md`, `DECISIONS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `README.md`, `LICENSE`, `.gitignore`

## Key Conventions
- **Styling**: CSS Modules only. Tokens via `var(--ren-*)`. No Tailwind, no inline colors, no inline pixels.
- **Strings**: All user-facing text → `src/i18n/locales/en.json`.
- **State (Rust)**: Only `RenStateMachine::transition()` or `force()` mutates state. Observers subscribe via `RenStateMachine::subscribe()` for in-process side effects.
- **State (Frontend)**: Only `useRenStore.setState()` — driven by `ren://state-changed` events.
- **Tauri Events**: `ren://state-changed`, `ren://transcript`, `ren://download-progress`, `ren://error`, `ren://waveform`.
- **Feature flags**: `stt` (whisper-rs), `tts` (ort + ndarray), `wake` (pv_porcupine). Default build links no native ML libraries.

## Build Status
- `npm run build` → ✅ clean
- `cargo check` (no features) → expected to compile on the work machine once deps download
- `cargo check --features stt` → needs C++ toolchain + CMake → home
- `cargo check --features tts` → needs ONNX Runtime native lib → home
- `cargo check --features wake` → needs Picovoice native lib + access key → home

## What's Needed at Home
- **Phase 2**: `cargo check --features stt`, download Whisper model, update SHA256 in `defaults.rs`, full PTT test.
- **Phase 3**: Download Ollama binary + Kokoro ONNX, implement `KokoroEngine::synthesize()` ORT inference, full voice loop test.
- **Phase 4**: Picovoice access key (`PICOVOICE_ACCESS_KEY` env var), train + bundle the two `.ppn` files, wire wake-engine consumer task into the audio pipeline, false-positive sanity check.

## What Phase 5 Starts With
Tool system: `Tool` trait, `ToolRegistry`, system / apps / Steam / files / web tool categories, frontend `ToolCard` component. See `PHASE-05-active.md`.
