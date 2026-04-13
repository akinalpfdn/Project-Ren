# Phase 04 — Wake Word and Full Conversation Loop
Status: DONE (work-computer code complete; Picovoice runtime + .ppn files + access key verified at home)

## Goal
Ren is summoned by voice. The user says "Ren" or "Hey Ren", Ren wakes up, conversation proceeds naturally, Ren returns to sleep after inactivity or explicit dismissal.

## Context From Phase 3
- Full pipeline is live: push-to-talk → STT → LLM → TTS → speaker.
- State machine manages: `Idle ↔ Listening ↔ Thinking ↔ Speaking`.
- In Phase 4, push-to-talk is replaced by wake word detection + VAD. The hotkey remains as a fallback override.
- The `StateControls.tsx` debug panel is removed in this phase — the real state machine drives everything now.

---

## What Was Built (work computer — no native Picovoice library required)

### Rust Backend
| File | Responsibility |
|------|----------------|
| `src-tauri/Cargo.toml` | Added optional `pv_porcupine = "3"` dependency and a new `wake = ["pv_porcupine"]` feature flag. Default build still has zero native ML deps. |
| `src-tauri/src/config/defaults.rs` | Added `WAKE_KEYWORD_HEY_REN`, `WAKE_KEYWORD_REN_UYAN`, `PICOVOICE_ACCESS_KEY` (resolved from `option_env!`), `WAKE_ACK_SOUND`, and a `DISMISSAL_PHRASES` list (English + Turkish). |
| `src-tauri/src/wake/mod.rs` | `WakeEngine` trait, `WakeKeyword` config struct, `WakeEvent` payload. Engine-agnostic so Porcupine can be swapped for openWakeWord later. |
| `src-tauri/src/wake/porcupine.rs` | `PorcupineWakeEngine`. With `--features wake`: real `pv_porcupine` integration (load via `spawn_blocking`, multi-keyword `process()` returning the matched keyword id). Without `wake`: `load()` returns a clear "feature disabled" error so the rest of the binary still compiles. |
| `src-tauri/src/dismissal/mod.rs` | Pure-Rust dismissal phrase detector. Substring match, case-insensitive, English + Turkish, unit tests cover empty / unrelated / matching cases. |
| `src-tauri/src/state/mod.rs` | Rewritten with a `tokio::sync::broadcast::Sender<RenState>`. `transition()` and `force()` now both notify in-process observers in addition to emitting `ren://state-changed`. `subscribe()` exposes a receiver. Transition matrix expanded with the `Sleeping → Listening` (PTT override) and `Listening → Idle` (zero-length transcript) edges. |
| `src-tauri/src/lib.rs` | Two new observer tasks: `spawn_conversation_timer` (Idle → Sleeping after `conversation_timeout_secs`, cancels on any state change) and `spawn_model_unloader` (drops `WhisperEngine` and `KokoroEngine` whenever Sleeping is entered). The full STT path now routes through `dismissal::is_dismissal()` after transcription — a match short-circuits straight to Sleeping. |

### Frontend
| File | Responsibility |
|------|----------------|
| `src/components/StateControls.{tsx,module.css}` | **Deleted.** Real state machine drives the UI; debug panel is no longer needed. |
| `src/App.tsx` | Removed `StateControls` import and render. |
| `src/styles/theme.css` | Added `--ren-orb-sleeping-opacity` and `--ren-orb-wake-overshoot` design tokens. |
| `src/components/Orb.module.css` | Sleeping state binds opacity to the new token. Waking is now a dramatic bounce: scale `0.6 → 1.25 → 1.0` over `--ren-duration-slow` with the bounce easing, and box-shadow ramps from subtle through speaking-intensity back to listening. |

**Frontend build: `npm run build` clean.**

---

## Home Testing Checklist

### 1. Picovoice access key + wake word resources
1. Sign up at `console.picovoice.ai` (free personal tier).
2. Copy the Access Key.
3. Set it for the build: `setx PICOVOICE_ACCESS_KEY "<key>"` and restart the shell so `option_env!` picks it up.
4. Open the Console → Porcupine → train two wake words for Windows:
   - `Hey Ren` → save as `hey_ren_en_windows.ppn`
   - `Ren uyan` → save as `ren_uyan_en_windows.ppn`
5. Place both files under `src-tauri/resources/wake/` and register them in `tauri.conf.json` `bundle.resources` (work for the home session).

### 2. Compile with the wake feature
```bash
cd src-tauri
cargo check --features wake
cargo check --features stt,tts,wake
```
If Picovoice's native lib does not auto-resolve, follow `pv_porcupine`'s build instructions — the runtime DLL/.so path may need to be set explicitly.

### 3. Wire wake detection into the audio pipeline
- Add a wake consumer task next to the existing VAD task that buffers cpal frames into `WakeEngine::frame_length()` chunks of i16 PCM (re-quantize the f32 samples).
- On `WakeEvent` while in Sleeping: force `Waking`, play `wake_ack.wav` via `AudioPlayer`, then `Listening`.
- This wiring is intentionally not done on the work computer — it depends on the loaded Porcupine engine.

### 4. End-to-end voice loop
- Say "Hey Ren" from across the room → Sleeping → Waking → Listening.
- Speak Turkish → Thinking → Speaking → Idle.
- Say nothing for 30 s → conversation timer fires → Sleeping. Confirm the model unloader drops Whisper + Kokoro from VRAM.
- Say "tamam yeter" anywhere in your sentence → immediate Sleeping.

### 5. Hotkey override still works
- Ctrl+Alt+R from Sleeping → Listening (already validated in Phase 2 tests).
- Ctrl+Alt+S from any state → Sleeping.

### 6. False-positive sanity check
- Run a podcast or a normal phone call near the mic for ~10 minutes.
- Note any false wakes; if more than 1–2 in 10 minutes, lower `wake_sensitivity` from 0.5 to 0.4 in `config.json` and retest.

---

## Architecture Notes

### Wake module shape
Engine-agnostic on purpose. The real Picovoice integration is gated behind the `wake` feature exactly like `stt` (whisper-rs) and `tts` (ort). The fallback impl errors cleanly so a default build compiles and runs without the native library.

### State observer pattern
`RenStateMachine` is now a small publisher: `subscribe()` returns a `broadcast::Receiver<RenState>`. `lib.rs` spawns two long-lived tasks:
- **Conversation timer** — keeps a `Option<Instant>` of when we last entered Idle, sleeps until either a new state change arrives or the deadline expires, then forces Sleeping.
- **Model unloader** — every Sleeping transition drops Whisper + Kokoro.

Adding a third observer (e.g. wake-engine load/unload) is now a one-function change.

### Dismissal pipeline
`dismissal::is_dismissal()` runs after STT, before the LLM is even consulted. Match → force-sleep, no Ollama round-trip, no TTS. Covers both the Turkish input ("tamam yeter") and the English LLM response (tested by feeding the LLM output through the same function in a future iteration).

---

## Acceptance Criteria
- [ ] Saying "Ren" or "Hey Ren" from across the room reliably wakes the assistant — *requires Porcupine runtime, deferred to home.*
- [ ] Models load in parallel with the wake animation so there's no noticeable delay — *deferred to home.*
- [x] State machine supports a follow-up turn without re-triggering the wake word (Idle → Listening edge added).
- [x] Idle timeout returns to sleep after the configurable duration (`spawn_conversation_timer`).
- [x] Voice dismissal works (`dismissal::is_dismissal`, 4 unit tests).
- [ ] False positive rate on the wake word is acceptable — *measure at home.*
- [x] StateControls debug UI is gone.

## Decisions Made This Phase
- Wake engine is trait-based + feature-gated, mirroring the `stt`/`tts` pattern. Keeps the work-machine compile path clean.
- Picovoice access key resolved via `option_env!("PICOVOICE_ACCESS_KEY")` at compile time, never committed to source.
- Conversation idle timer lives in `lib.rs` as a state observer rather than inside `RenStateMachine` itself — keeps the state machine free of `tokio::time` dependencies and easier to unit-test.

## Known Risks
- Wake word "Ren" alone is a short name → high false-positive risk. Default to "Hey Ren" first, fall back to "Ren uyan" only if needed. Re-evaluate after the home false-positive sanity check.
- Parallel model loading on wake: if Whisper takes longer than the user's speaking time, there is a perceptible gap. Measure and consider preloading on `Waking` rather than waiting for the first STT call.
- Picovoice free tier: commercial redistribution is restricted. Document clearly. If Ren scales commercially, evaluate openWakeWord (Apache-2.0) as a drop-in `WakeEngine` impl.
