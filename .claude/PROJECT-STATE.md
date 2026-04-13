# Project Ren — Current State
Last updated: 2026-04-13

## Active Phase
**Phase 3** — LLM + TTS (code complete, needs home testing)

## Phase Files
| File | Status |
|------|--------|
| `.claude/phases/PHASE-01-complete.md` | COMPLETE |
| `.claude/phases/PHASE-02-active.md` | ACTIVE (code done, home test pending) |
| `.claude/phases/PHASE-03.md` | ACTIVE ← you are here |
| `.claude/phases/PHASE-04.md` | PENDING |
| `.claude/phases/PHASE-05.md` | PENDING |
| `.claude/phases/PHASE-06.md` | PENDING |
| `.claude/phases/PHASE-07.md` | PENDING |

## What Exists in the Repo Right Now

### Rust Backend (`src-tauri/src/`)
- `config/defaults.rs` — All constants (ports, sample rates, filenames, hashes)
- `config/mod.rs` — AppConfig, load/save, path helpers (app_data_dir, models_dir, etc.)
- `state/mod.rs` — RenStateMachine, 8 states, transition validation, Tauri event emit
- `audio/capture.rs` — cpal 16kHz mono mic capture → channel
- `audio/vad.rs` — VAD task, SpeechStart/SpeechEnd events, 500ms silence threshold
- `audio/mod.rs` — start_pipeline() combining capture + VAD
- `stt/mod.rs` — SttEngine trait
- `stt/whisper.rs` — WhisperEngine, lazy load, feature-gated on `stt`
- `download/verify.rs` — SHA256 verification
- `download/mod.rs` — Resumable HTTP download, .part files, progress events
- `hotkey/mod.rs` — Ctrl+Alt+R (PTT), Ctrl+Alt+S (force sleep)
- `commands/mod.rs` — toggle/show/hide window, get_state Tauri commands
- `lib.rs` — Full wiring: init, state machine, tray, audio, hotkeys, event loop

### Frontend (`src/`)
- `types/index.ts` — RenState + all Tauri event payload types
- `store/index.ts` — Zustand: currentState, error, transcript, downloadProgress
- `hooks/useRenEvents.ts` — Tauri event listeners → store
- `components/Orb.tsx` + `Orb.module.css` — State animations
- `components/Transcript.tsx` + `.module.css` — STT transcript display
- `components/DownloadOverlay.tsx` + `.module.css` — Download progress UI
- `components/StateControls.tsx` + `.module.css` — DEBUG ONLY, remove in Phase 4
- `styles/theme.css` — All design tokens
- `styles/global.css` — Resets
- `i18n/index.ts` — i18n init
- `i18n/locales/en.json` — English strings
- `App.tsx` — Root: useRenEvents, Orb, Transcript, DownloadOverlay

### Project Files
- `DEVPLAN.md`, `DECISIONS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `README.md`, `LICENSE`
- `.gitignore` — Rust + Node + Tauri

## Key Conventions
- **Styling**: CSS Modules only. Variables from `var(--ren-*)`. No Tailwind, no inline colors.
- **Strings**: All user-facing → `src/i18n/locales/en.json`
- **State (Rust)**: Only `RenStateMachine::transition()` or `force()` mutates state
- **State (Frontend)**: Only `useRenStore().setState()` — driven by `ren://state-changed` events
- **Tauri Events**: `ren://state-changed`, `ren://transcript`, `ren://download-progress`, `ren://error`
- **`stt` feature flag**: Whisper-rs only compiles/links when `--features stt` is passed. Default build has no C++ deps.

## Build Status
- `npm run build` → ✅ clean
- `cargo check` (no features) → will compile on work machine once deps download
- `cargo check --features stt` → needs C++ toolchain + CMake → test at home

## What's Needed at Home (phases 2 & 3)
- **Phase 2**: `cargo check --features stt`, download Whisper model, update SHA256 in defaults.rs, full PTT test
- **Phase 3**: Download Ollama binary + Kokoro ONNX, implement `KokoroEngine::synthesize()` ORT inference, full voice loop test

## Rust Backend — New in Phase 3
- `llm/ollama_process.rs` — child process + Windows Job Object + port probing
- `llm/client.rs` — OllamaClient with SSE streaming
- `llm/prompt.rs` — JARVIS system prompt
- `llm/conversation.rs` — per-session history
- `llm/mod.rs` — `run_turn()` + sentence boundary splitter
- `tts/mod.rs` — TtsEngine trait
- `tts/kokoro.rs` — KokoroEngine (feature-gated, ORT inference TODO for home)
- `playback/mod.rs` — AudioPlayer (rodio), waveform RMS computation

## What Phase 4 Starts With
Wake word (Porcupine), proper state machine overhaul, remove StateControls debug UI.
See `PHASE-04.md`.
