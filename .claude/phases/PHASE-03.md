# Phase 03 — Local LLM via Portable Ollama, and TTS
Status: ACTIVE (code complete, needs home testing)

## Goal
Ren understands transcribed speech, generates English responses via a private Ollama child process running Qwen 2.5 14B, and speaks them out loud via Kokoro TTS.

---

## ✅ Done (work computer — no model/runtime needed)

### Rust Backend
| File | What it does |
|------|-------------|
| `src-tauri/Cargo.toml` | Added: `rodio`, `ort` (optional/`tts` feature), `ndarray` (optional), `unicode-segmentation`. New feature flag: `tts = ["ort", "ndarray"]` |
| `src-tauri/src/llm/ollama_process.rs` | `start()` — spawns `ollama serve` as child process. Port probing (11500–11520). `OLLAMA_MODELS` env override. Windows Job Object via `windows-rs` so child dies with parent. `health_check()` polls `/api/tags`. `terminate()` for clean shutdown. `ollama_exe_path()`, `ollama_download_url()` helpers |
| `src-tauri/src/llm/client.rs` | `OllamaClient` — `chat_stream()` POST `/api/chat` with SSE streaming. Parses token deltas and tool calls from JSON chunks. `ping()` for keep-alive. `Message` struct with system/user/assistant/tool_result constructors |
| `src-tauri/src/llm/prompt.rs` | `build_system_prompt()` — JARVIS personality, Turkish→English rule, no filler phrases, tool use guidance. Inline `const SYSTEM_PROMPT_BASE` |
| `src-tauri/src/llm/conversation.rs` | `Conversation` — Vec<Message> with system prompt at [0]. `push_user`, `push_assistant`, `push_tool_result`, `reset()`, `messages()` |
| `src-tauri/src/llm/mod.rs` | `run_turn()` — drives full LLM turn: appends user msg, streams tokens, splits on sentence boundaries, sends to `sentence_tx`. `find_sentence_boundary()`. `default_client()` |
| `src-tauri/src/tts/mod.rs` | `TtsEngine` trait: `synthesize(&str)`, `load()`, `unload()`, `is_loaded()`, `sample_rate()` |
| `src-tauri/src/tts/kokoro.rs` | `KokoroEngine` — feature-gated on `tts`. `load()` → ORT Session from `%APPDATA%\Ren\models\kokoro\kokoro.onnx`. `synthesize()` stub with TODO comment for ORT inference (complete at home). `sample_rate()` = 24000 |
| `src-tauri/src/playback/mod.rs` | `AudioPlayer` — rodio `OutputStream` + `Sink`. `play()` takes `AudioBuffer` + `sample_rate`, emits `ren://waveform` with 8-bar RMS amplitudes before/after playback. `compute_waveform()` |
| `src-tauri/src/lib.rs` | Full Phase 3 wiring: `sentence_tx/rx` channel, Ollama start (non-fatal if missing), `tts_sentence_loop` task (lazy Kokoro load → Speaking state → playback → Idle), `run_full_turn` (STT → transcript event → LLM if Ollama running → sentence stream) |

### Frontend
| File | What it does |
|------|-------------|
| `src/types/index.ts` | Added `WaveformPayload { amplitudes: number[] }` |
| `src/store/index.ts` | Added `waveformAmplitudes: number[]` field + `setWaveform()` action |
| `src/hooks/useRenEvents.ts` | Added `ren://waveform` listener → `setWaveform()` |
| `src/components/Orb.tsx` | Speaking state now uses real `waveformAmplitudes` from store. Each bar's `scaleY` = `max(0.15, amplitude)` — data-driven instead of CSS keyframe |

**Frontend build: ✅ passes clean**

---

## 🏠 Eve Gidince Yapılacaklar

### 1. Ollama binary'yi indir
```
# Otomatik indirme Phase 7'de first-run wizard'a eklenecek.
# Şimdilik manual:
# URL: https://github.com/ollama/ollama/releases/download/v0.9.0/ollama-windows-amd64.exe
# Koy: %APPDATA%\Ren\bin\ollama.exe
```

### 2. Ollama child process test
```bash
npm run tauri dev
# Konsolda "Ollama ready on port 11500" görünmeli
# Task Manager'da ollama.exe görünmeli
# Ren kapatınca ollama.exe da kapanmalı (Job Object test)
```

### 3. Qwen 14B pull et
```bash
# Ren'in private Ollama'sı çalışırken:
%APPDATA%\Ren\bin\ollama.exe pull qwen2.5:14b
# Ya da Ren başlarken otomatik pull — Phase 7'de first-run wizard halleder
```

### 4. LLM turu test
- Ctrl+Alt+R → Türkçe konuş → transcript → Ollama cevap vermeli
- Cevap `sentence_tx` üzerinden akmalı, konsol loglarında token'lar görünmeli
- State: Thinking → (Speaking — Kokoro hazır olunca)

### 5. Kokoro ONNX modelini indir
```
# Kaynak: https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX
# Dosya: kokoro.onnx (~300MB)
# Koy: %APPDATA%\Ren\models\kokoro\kokoro.onnx
```

### 6. ORT inference'ı tamamla (tts/kokoro.rs)
Mevcut `synthesize()` fonksiyonundaki TODO'yu doldur:
- Kokoro tokenizer: https://github.com/thewh1teagle/kokoro-onnx için referans Rust impl bak
- Input tensors: `input_ids` (phoneme IDs), `style` (voice embedding for `bf_emma`), `speed` (1.0)
- Output: audio samples → `AudioBuffer`

### 7. Ses testi
```bash
cd src-tauri
cargo build --features stt,tts
# Full pipeline: Türkçe konuş → İngilizce cevap gelmeli
```

### 8. Orphan process test
- Ren'i task manager'dan zorla kapat
- `ollama.exe`'nin de kapandığını doğrula (Job Object çalışıyor mu?)

### 9. Port conflict test
- Port 11500'ü başka bir şeyle meşgul et, Ren'in 11501'e geçtiğini doğrula

---

## Acceptance Criteria Durumu
- [ ] Ren downloads Ollama binary — **Phase 7 first-run wizard**
- [ ] Ollama child starts on custom port — **✅ Rust kodu tamam, eve test**
- [ ] Ren responds to Turkish speech with English speech — **eve test (Kokoro synthesize TODO)**
- [ ] Personality consistent — **✅ system prompt yazıldı**
- [ ] Time-to-first-audio under 2s — **eve ölç**
- [ ] Conversation history works — **✅ Conversation struct**
- [ ] Killing Ren terminates Ollama — **✅ Job Object kodu tamam, eve test**
- [ ] Pre-existing system Ollama doesn't interfere — **✅ port isolation + OLLAMA_MODELS override**
- [ ] Downloads resumable — **Phase 7**
