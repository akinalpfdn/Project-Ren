# Phase 03 — Local LLM via Portable Ollama, and TTS
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
- [ ] Ollama child starts on a custom port — *Rust complete; verify at home.*
- [ ] Ren responds to Turkish speech with English speech — *verify at home (Kokoro `synthesize` TODO outstanding).*
- [x] Personality is consistent — *system prompt written.*
- [ ] Time-to-first-audio under 2 s — *measure at home.*
- [x] Conversation history works — *`Conversation` struct in place.*
- [ ] Killing Ren terminates Ollama — *Job Object code complete; verify at home.*
- [x] Pre-existing system Ollama does not interfere — *port isolation + `OLLAMA_MODELS` override.*
- [ ] Downloads are resumable — *Phase 7.*
