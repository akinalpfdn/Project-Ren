# Phase 02 — Audio Pipeline Foundation
Status: ACTIVE

## Goal
Ren can capture audio from the microphone, detect speech, transcribe it locally with Whisper, and display the transcript on screen.

---

## ✅ Done (work computer — no model needed)

### Rust Backend
| File | What it does |
|------|-------------|
| `src-tauri/Cargo.toml` | Added: `cpal`, `voice_activity_detector`, `whisper-rs` (optional/`stt` feature), `global-hotkey`, `reqwest`, `sha2`, `hex`, `directories`, `tokio`, `tracing`, `anyhow`, `thiserror`, `futures`, `async-trait`, `windows` |
| `src-tauri/src/config/defaults.rs` | All constants: sample rate, VAD timing, Ollama ports, model filenames, SHA256 hashes, download chunk size |
| `src-tauri/src/config/mod.rs` | `AppConfig` struct (serde, default impl), `load()`/`save()`, path helpers: `app_data_dir()`, `models_dir()`, `logs_dir()`, `bin_dir()`, `cache_dir()` |
| `src-tauri/src/state/mod.rs` | `RenStateMachine`: all 8 states, `transition()` (validates legal pairs), `force()`, `emit_error()`. Emits `ren://state-changed`. `SharedStateMachine = Arc<Mutex<...>>` |
| `src-tauri/src/audio/capture.rs` | `start_capture(frame_ms)` — cpal 16kHz mono f32 → `mpsc::Sender<AudioSamples>`. Fallback config if device doesn't support 16kHz natively |
| `src-tauri/src/audio/vad.rs` | `run()` async task — consumes audio frames, runs `voice_activity_detector`, emits `VadEvent::SpeechStart` / `VadEvent::SpeechEnd(buffer)` after 500ms silence |
| `src-tauri/src/audio/mod.rs` | `start_pipeline()` — starts capture + spawns VAD task, returns cpal Stream handle |
| `src-tauri/src/stt/mod.rs` | `SttEngine` trait: `load()`, `unload()`, `transcribe(&[f32])`, `is_loaded()` |
| `src-tauri/src/stt/whisper.rs` | `WhisperEngine` — feature-gated on `stt`. Lazy load via `spawn_blocking`. Turkish (`tr`) language param. `#[cfg(not(feature = "stt"))]` path returns clear error |
| `src-tauri/src/download/verify.rs` | `verify_sha256()`, `is_valid_download()` |
| `src-tauri/src/download/mod.rs` | `download_file()` — HTTP Range resume, chunk write to `.part` file, rename on complete, SHA256 verify, emits `ren://download-progress` |
| `src-tauri/src/hotkey/mod.rs` | `start()` — registers `Ctrl+Alt+R` (push-to-talk) and `Ctrl+Alt+S` (force sleep), returns `GlobalHotKeyManager`. Async listener loop, sends `HotkeyEvent` channel |
| `src-tauri/src/commands/mod.rs` | `toggle_window`, `show_window`, `hide_window`, `get_state` Tauri commands |
| `src-tauri/src/lib.rs` | Full wiring: logging init, config load, state machine, tray, audio pipeline, hotkey manager, Whisper Arc<Mutex<>>, main `event_loop` tokio task. Push-to-talk + VAD → state transitions → transcription → `ren://transcript` event |

### Frontend
| File | What it does |
|------|-------------|
| `src/types/index.ts` | Added: `StateChangedPayload`, `TranscriptPayload`, `DownloadProgressPayload`, `ErrorPayload` |
| `src/store/index.ts` | Added: `transcript`, `downloadProgress` fields + setters |
| `src/hooks/useRenEvents.ts` | `useRenEvents()` hook — subscribes to all 4 Tauri events, maps to store actions |
| `src/components/Transcript.tsx` + `.module.css` | Fades in transcript text below orb, auto-clear after 8s |
| `src/components/DownloadOverlay.tsx` + `.module.css` | Full-screen overlay during downloads: step name, progress bar, bytes + speed |
| `src/App.tsx` | Calls `useRenEvents()`, renders `Transcript`, `DownloadOverlay` (shown when `downloadProgress` != null) |

**Frontend build: ✅ passes clean**

---

## 🏠 Eve Gidince Yapılacaklar

### 1. Whisper feature ile compile test
```bash
# Requires: Visual Studio Build Tools + CMake
cd src-tauri
cargo check --features stt
```
- Eğer hata alırsan: `whisper-rs` build için CMake'in PATH'te olması lazım. `winget install Kitware.CMake` ile ekle.

### 2. İlk gerçek audio capture testi
```bash
npm run tauri dev
```
- Ctrl+Alt+R'ye bas, sus, bırak → state: idle → listening → thinking → idle geçmeli
- Konsol loglarında `"Audio capture started"`, `"VAD: speech started"`, `"VAD: speech ended"` görünmeli
- `stt` feature olmadan Whisper load error gelecek — bu beklenen davranış

### 3. Whisper model indir
```bash
# %APPDATA%\Ren\models\whisper\ klasörüne koy
# Kaynak: https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin
# SHA256: 964ef9a7b601b6847c71ba5d2d0f7e4f41cd5eed99b86e73c9b0bd0e9f69c8ec (VERIFY!)
```

### 4. Whisper SHA256 hash'ini güncelle
- `src-tauri/src/config/defaults.rs` → `WHISPER_LARGE_V3_SHA256` sabitini gerçek hash'le güncelle
- Hash'i `certutil -hashfile ggml-large-v3.bin SHA256` ile doğrula

### 5. STT feature ile tam test
```bash
cd src-tauri
cargo build --features stt  # ~20 min ilk seferinde (whisper.cpp derleniyor)
npm run tauri dev -- -- --features stt  # varsa bu çalışmıyor, tauri.conf.json'a feature ekle
```
**Not**: whisper-rs feature'ı Tauri dev ile kullanmak için `src-tauri/.cargo/config.toml` dosyası gerekebilir:
```toml
[build]
features = ["stt"]
```

### 6. Türkçe transkripsiyon kalitesini test et
- Farklı söz varlığı: "Ren, müzik aç", "Bugün hava nasıl", "Saati kaç" gibi kısa komutlar
- Eğer kısa utterance'larda sorun varsa: WhisperEngine'de `p.set_prompt()` ile Türkçe komut prefix'i ekle

### 7. Download progress UI test
- `useRenStore.getState().setDownloadProgress({ step: "whisper", downloadedBytes: 500000000, totalBytes: 3000000000, speedBps: 10000000 })` ile overlay'i test et (browser console'dan)

---

## Acceptance Criteria Durumu
- [ ] First launch shows download screen — **DownloadOverlay bitti, Rust download trigger'ı Phase 7'de**
- [ ] Holding Ctrl+Alt+R captures audio, releasing transcribes it — **Rust kodu tamam, eve test**
- [ ] Turkish speech accurately transcribed — **eve test**
- [ ] Transcript appears in UI below orb — **✅ Transcript component bitti**
- [ ] State transitions correctly reflect audio pipeline — **✅ State machine + events bitti**
- [ ] Model load lazy on first push-to-talk — **✅ WhisperEngine.load() sadece transcribe öncesi çağrılıyor**
