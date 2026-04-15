# Phase 03 — Local LLM via Portable Ollama, and TTS
Status: ACTIVE — reopened to resolve in-process CUDA conflict (Whisper + ORT) by extracting Kokoro into a sidecar binary modeled on Ollama's process pattern.

## Reopen Reason (2026-04-15)
Running `_dev-full.bat` (`stt,tts,wake`) crashes with a CRT debug assertion (`_osfile(fh) & FOPEN`) followed by `STATUS_STACK_BUFFER_OVERRUN`. Root cause: whisper-rs (whisper.cpp CUDA backend) and `ort` (downloaded ONNX Runtime DLL with CUDA EP bundled) both initialise CUDA in the same process; their CRT/ABI assumptions are incompatible.

## Plan — Kokoro Sidecar
Mirror the existing Ollama-as-child-process architecture:

1. Convert `src-tauri/` into a Cargo workspace with two member crates:
   - `crates/ren-app/` — current Tauri app, **without** ORT or `kokoro-tiny`.
   - `crates/ren-tts/` — new minimal binary owning Kokoro.
2. `ren-tts` exposes a localhost HTTP API:
   - `POST /synthesize` `{ "text": ..., "voice": ... }` -> `audio/octet-stream` raw `f32` PCM, `X-Sample-Rate` header.
   - `GET /health` -> 200 OK once the model is loaded.
3. Lifecycle copies `llm/ollama_process.rs`:
   - Random localhost port pick (avoid 11500-11520 used by Ollama).
   - Windows Job Object so the child dies with the parent.
   - Health-check polling, ready signal feeds the existing TTS path.
4. `ren-app/src/tts/` gains an `HttpKokoroClient` that implements `TtsEngine` and talks to the sidecar.
5. Models stay shared: both binaries read from `%APPDATA%\Ren\models\kokoro\`.
6. `tauri.conf.json` lists `ren-tts.exe` under `externalBin`; build copies it next to `ren.exe`.
7. Dev scripts:
   - `_dev-full.bat` for `ren-app` no longer needs `tts` feature, only `stt,wake`.
   - `_dev-tts.bat` (new) builds and runs the sidecar standalone for direct testing.

## Tasks
- [ ] Workspace refactor (`Cargo.toml` root + members, move existing code into `crates/ren-app/`).
- [ ] `crates/ren-tts/` skeleton (Cargo.toml, `main.rs`, axum server stub).
- [ ] Move Kokoro engine + model loading into `ren-tts`.
- [ ] Implement `/synthesize` and `/health` endpoints.
- [ ] Strip `ort`, `ndarray`, `kokoro-tiny` from `ren-app/Cargo.toml` and the `tts` feature.
- [ ] New `HttpKokoroClient` in `ren-app/src/tts/http.rs`.
- [ ] Sidecar lifecycle in `ren-app/src/lib.rs` setup hook (spawn + Job Object + health-check + terminate).
- [ ] `tauri.conf.json` `externalBin` entry + dev script updates.
- [ ] End-to-end test: STT (CUDA) + LLM (Ollama) + TTS (sidecar) + Wake — full voice loop.

## Acceptance Criteria
- `_dev-full.bat` boots without CRT/GS-cookie crash.
- Voice loop completes a full turn end-to-end on the home machine.
- Killing `ren.exe` from Task Manager terminates `ren-tts.exe` automatically.
- Running `ren-tts.exe` standalone responds to `curl -X POST http://localhost:<port>/synthesize`.

---

## (Pre-Reopen) — original Phase 03 notes
Status: COMPLETE (code complete on work computer; runtime + model verification pending at home)

## Goal
Ren understands transcribed speech, generates English responses via a private Ollama child process running Qwen 2.5 14B, and speaks them out loud via Kokoro TTS.

---

## What Was Built

### Rust Backend
| File | Responsibility |
|------|----------------|
| `src-tauri/Cargo.toml` | Added `rodio`, `ort` (optional, gated on `tts`), `ndarray` (optional), `unicode-segmentation`. New feature flag: `tts = ["ort", "ndarray"]` |
| `src-tauri/src/llm/ollama_process.rs` | `start()` — spawns `ollama serve` as a child process. Port probing across 11500–11520. `OLLAMA_MODELS` env override. Windows Job Object via `windows-rs` so the child dies with the parent. `health_check()` polls `/api/tags`. `terminate()` for clean shutdown. `ollama_exe_path()` and `ollama_download_url()` helpers |
| `src-tauri/src/llm/client.rs` | `OllamaClient` — `chat_stream()` POSTs `/api/chat` with SSE streaming. Parses token deltas and tool calls from JSON chunks. `ping()` for keep-alive. `Message` struct with `system` / `user` / `assistant` / `tool_result` constructors |
| `src-tauri/src/llm/prompt.rs` | `build_system_prompt()` — JARVIS personality, Turkish→English rule, no filler phrases, tool-use guidance. Inline `const SYSTEM_PROMPT_BASE` |
| `src-tauri/src/llm/conversation.rs` | `Conversation` — `Vec<Message>` with the system prompt fixed at index 0. `push_user`, `push_assistant`, `push_tool_result`, `reset()`, `messages()` |
| `src-tauri/src/llm/mod.rs` | `run_turn()` — drives a full LLM turn: appends the user message, streams tokens, splits on sentence boundaries, sends each sentence to `sentence_tx`. Helpers: `find_sentence_boundary()`, `default_client()` |
| `src-tauri/src/tts/mod.rs` | `TtsEngine` trait: `synthesize(&str)`, `load()`, `unload()`, `is_loaded()`, `sample_rate()` |
| `src-tauri/src/tts/kokoro.rs` | `KokoroEngine` — gated on the `tts` feature. `load()` opens an ORT Session from `%APPDATA%\Ren\models\kokoro\kokoro.onnx`. `synthesize()` is a stub with a `TODO` for the ORT inference (to be completed at home). `sample_rate()` = 24000 |
| `src-tauri/src/playback/mod.rs` | `AudioPlayer` — rodio `OutputStream` + `Sink`. `play()` takes an `AudioBuffer` + `sample_rate`, emits `ren://waveform` with the 8-bar RMS amplitudes both before and after playback. `compute_waveform()` |
| `src-tauri/src/lib.rs` | Full Phase 3 wiring: `sentence_tx` / `sentence_rx` channel, Ollama start (non-fatal if the binary is missing), `tts_sentence_loop` task (lazy Kokoro load → `Speaking` state → playback → `Idle`), `run_full_turn` (STT → transcript event → LLM if Ollama is running → sentence stream) |

### Frontend
| File | Responsibility |
|------|----------------|
| `src/types/index.ts` | Added `WaveformPayload { amplitudes: number[] }` |
| `src/store/index.ts` | Added `waveformAmplitudes: number[]` field with `setWaveform()` action |
| `src/hooks/useRenEvents.ts` | Added a `ren://waveform` listener that calls `setWaveform()` |
| `src/components/Orb.tsx` | The Speaking state now consumes real `waveformAmplitudes` from the store. Each bar's `scaleY` = `max(WAVEFORM_MIN_SCALE, amplitude)` — data-driven, not a CSS keyframe |

**Frontend build: `npm run build` clean.**

---

## Home Testing Checklist

### 1. Download the Ollama binary
- Automatic download lands in the Phase 7 first-run wizard. For now, place it manually:
- URL: `https://github.com/ollama/ollama/releases/download/v0.9.0/ollama-windows-amd64.exe`
- Path: `%APPDATA%\Ren\bin\ollama.exe`

### 2. Verify the Ollama child process
```bash
npm run tauri dev
```
- Console should log `"Ollama ready on port 11500"`.
- Task Manager should show `ollama.exe` running.
- Closing Ren must terminate `ollama.exe` as well — this validates the Job Object binding.

### 3. Pull the Qwen 2.5 14B model
While Ren's private Ollama is running:
```bash
%APPDATA%\Ren\bin\ollama.exe pull qwen2.5:14b
```
The Phase 7 first-run wizard will eventually drive this automatically.

### 4. End-to-end LLM turn
- Press Ctrl+Alt+R, speak Turkish → transcript event → Ollama responds.
- Tokens stream through `sentence_tx`; check console logs.
- States walk: `Listening` → `Thinking` → (`Speaking` once Kokoro is wired up).

### 5. Download the Kokoro ONNX model
- Source: `https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX`
- File: `kokoro.onnx` (~300 MB)
- Path: `%APPDATA%\Ren\models\kokoro\kokoro.onnx`

### 6. Complete the ORT inference in `tts/kokoro.rs`
Fill in the `TODO` inside `synthesize()`:
- Tokenizer reference (Rust): `https://github.com/thewh1teagle/kokoro-onnx`
- Input tensors: `input_ids` (phoneme IDs), `style` (voice embedding for `bf_emma`), `speed` (1.0)
- Output: audio samples → `AudioBuffer`

### 7. Full audio loop test
```bash
cd src-tauri
cargo build --features stt,tts
# Full pipeline: speak Turkish → English voice response.
```

### 8. Orphan-process test
- Force-kill Ren from Task Manager.
- Confirm `ollama.exe` is also gone (Job Object behaviour).

### 9. Port-conflict test
- Occupy port 11500 with another process.
- Confirm Ren falls through to 11501 (or the next free port in the probe range).

---

## Acceptance Criteria
- [ ] Ren downloads the Ollama binary — *deferred to the Phase 7 first-run wizard.*
- [x] Ollama child starts on a custom port — *verified: Ren spawns Ollama on 11500, log confirms `"Ollama ready on port 11500"`.*
- [x] `KokoroEngine::synthesize()` implemented — *wraps `kokoro-tiny` 0.1 (espeak-rs phonemizer + ORT inference + voice embeddings); compiles clean with `--features tts`.*
- [ ] Ren responds to Turkish speech with English speech — *blocked: remote-session limitation + ORT-CUDA vs whisper.cpp-CUDA conflict triggering `STATUS_STACK_BUFFER_OVERRUN` on Whisper model load. **NEEDS PHYSICAL ACCESS + CUDA-sharing fix.***
- [x] Personality is consistent — *system prompt written.*
- [ ] Time-to-first-audio under 2 s — *measure at home after CUDA conflict resolved.*
- [x] Conversation history works — *`Conversation` struct in place.*
- [ ] Killing Ren terminates Ollama — *Job Object code complete; **needs physical Task-Manager kill test.***
- [x] Pre-existing system Ollama does not interfere — *port isolation + `OLLAMA_MODELS` override.*
- [ ] Downloads are resumable — *Phase 7.*

### Remaining home-only work
1. **CUDA backend conflict** — ORT (via kokoro-tiny + `download-binaries`) and whisper.cpp both initialize CUDA contexts. Currently triggers a CRT `_osfile(fh) & FOPEN` debug assert followed by stack-buffer-overrun on Whisper load. Options when back on hardware:
   - Disable kokoro-tiny's `cuda` feature (already attempted — crash persists, so issue is static-link + DllMain init, not runtime usage).
   - Serialize the two backends: load-unload pattern, or run TTS in a separate process.
   - Switch Whisper to an ORT-based engine so both share the ORT runtime.
2. **Orphan-process test** — force-kill Ren from Task Manager, confirm `ollama.exe` dies via Job Object.
3. **Port-conflict test** — occupy 11500, confirm Ren probes up to 11520.
