# Phase 03 — Local LLM via Portable Ollama, and TTS
Status: PENDING (depends on Phase 2)

## Goal
Ren understands transcribed speech, generates English responses via a private Ollama child process running Qwen 2.5 14B, and speaks them out loud via Kokoro TTS.

## Context From Phase 2
- By end of Phase 2, Turkish transcript arrives via `ren://transcript` event with `isFinal: true`.
- State machine handles: idle → listening → thinking → idle. In Phase 3, "thinking" expands to: STT complete → LLM inference → TTS synthesis → speaking → idle.
- The `stt/mod.rs` STT trait established in Phase 2 should be followed as a model for the LLM and TTS traits.

## Tasks

### Ollama Child Process Manager
- [ ] Download manager for `ollama.exe`: detect if `%APPDATA%\Ren\bin\ollama.exe` exists; if not, download from pinned GitHub release URL, SHA256 verify
- [ ] `OllamaProcess` struct in `src-tauri/src/llm/ollama_process.rs`: spawns `ollama serve` as child process with:
  - `OLLAMA_MODELS` env var → `%APPDATA%\Ren\models\ollama\`
  - `OLLAMA_HOST` env var → `127.0.0.1:<selected_port>`
- [ ] Port selection: try port 11500 → if occupied, probe 11501–11520 → store chosen port in session config
- [ ] Windows Job Object: bind child process to parent so OS kills child if Ren crashes (prevents orphaned `ollama.exe` in Task Manager)
- [ ] Health check loop: poll `GET /api/tags` every 500ms until 200 OK or 30s timeout
- [ ] Process lifecycle: clean shutdown on `AppExit` event; if Ren crashes, Job Object handles it
- [ ] Pre-run: `ollama pull qwen2.5:14b` via child's HTTP API, stream download progress to `ren://download-progress`

### LLM HTTP Client
- [ ] `OllamaClient` struct in `src-tauri/src/llm/client.rs`: wraps `reqwest`, targets `http://127.0.0.1:<port>`
- [ ] `chat()` method: POST `/api/chat`, streaming response via SSE, returns `impl Stream<Item = String>`
- [ ] Conversation history: `Vec<Message>` maintained in memory per session, passed with each request
- [ ] System prompt: defined in `src-tauri/src/llm/prompt.rs` — JARVIS personality, calm and dry, addresses user as "sir" occasionally, Turkish input → English output, tool schemas appended in Phase 5

### TTS Engine
- [ ] Kokoro ONNX model download: `%APPDATA%\Ren\models\kokoro\kokoro.onnx` (~300MB from HuggingFace `onnx-community/Kokoro-82M-v1.0-ONNX`)
- [ ] `KokoroEngine` in `src-tauri/src/tts/kokoro.rs`: wraps `ort` (ONNX Runtime), default voice `bf_emma`
- [ ] `TtsTrait` in `src-tauri/src/tts/mod.rs` — abstracted so voice engine can be swapped later (e.g. for Turkish TTS or Piper)
- [ ] Audio playback: `rodio` for speaker output
- [ ] Waveform data: emit audio amplitude chunks via `ren://waveform` for speaking animation

### Streaming Pipeline
- [ ] Sentence-level chunking: as LLM tokens arrive, buffer until sentence boundary (`.`, `?`, `!`), send chunk to TTS immediately — minimizes time-to-first-audio
- [ ] Pipeline: transcript → LLM stream → sentence splitter → TTS → playback + waveform events

### Frontend
- [ ] Speaking state: Orb's waveform animation driven by real `ren://waveform` amplitude data (replace CSS keyframe with data-driven bars)
- [ ] Remove placeholder transcript display from Phase 2 if it's just debug; keep if it looks good
- [ ] Handle `ren://waveform` event in store/listener

## Architecture Notes

### Crate Structure Additions
```
src-tauri/src/
  llm/
    mod.rs              — LlmTrait definition
    client.rs           — OllamaClient (reqwest HTTP wrapper)
    ollama_process.rs   — OllamaProcess (child process manager)
    prompt.rs           — System prompt builder
  tts/
    mod.rs              — TtsTrait definition
    kokoro.rs           — KokoroEngine (ort/ONNX)
  config/
    mod.rs              — AppConfig struct (reads/writes %APPDATA%\Ren\config.json)
    defaults.rs         — All default values as constants
```

### Config Defaults (establish here, use throughout)
- Ollama preferred port: `11500`
- Ollama port probe range: `11500–11520`
- Ollama keep_alive: `"30m"`
- Ollama model: `"qwen2.5:14b"`
- Default TTS voice: `"bf_emma"`
- Conversation idle timeout: `30` seconds (used in Phase 4)

### System Prompt Skeleton
```
You are Ren, a calm, dry, highly capable personal AI assistant running entirely on the user's machine.
You respond in English only, even when spoken to in Turkish.
Your personality is inspired by JARVIS: efficient, composed, occasionally dry humor, address the user as "sir" when appropriate.
Responses are concise — never verbose unless the user asks for detail.
You have access to tools. When a user request maps to a tool, call it directly without asking for confirmation unless the action is destructive.
```

### New Tauri Events
- `ren://waveform` — payload: `{ amplitudes: number[] }` (array of 8 bar heights, 0.0–1.0)
- `ren://llm-token` — payload: `{ token: string }` (optional, for showing live typing)

## Acceptance Criteria
- [ ] Ren downloads Ollama binary and Qwen 14B on first launch with progress UI
- [ ] Ollama child process starts on a custom port and survives until Ren exits
- [ ] Ren responds to Turkish speech with English speech
- [ ] Personality is consistent across turns
- [ ] Time-to-first-audio under 2 seconds on target hardware after warm-up
- [ ] Conversation history works for follow-up questions within a session
- [ ] Killing Ren cleanly terminates the child Ollama process — no orphans in Task Manager
- [ ] Pre-existing system Ollama (if user has one) does not interfere with Ren's instance
- [ ] Downloads are resumable if interrupted

## Decisions Made This Phase
<!-- Append here as they happen -->

## Known Risks
- Qwen 14B + Whisper large-v3 together may push 12GB VRAM limit. Monitor real usage; fall back to Whisper medium if OOM.
- Kokoro is English-only TTS — Turkish TTS output not supported. TtsTrait abstraction ensures this can be swapped later.
- Windows Job Object for child process requires careful handle management — test crash scenarios explicitly.
- Ollama binary version must be pinned — define pinned version constant in `config/defaults.rs`.
