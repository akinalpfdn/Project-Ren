# Phase 04 — Wake Word and Full Conversation Loop
Status: PENDING (depends on Phase 3)

## Goal
Ren is summoned by voice. User says "Ren" or "Hey Ren", Ren wakes up, conversation proceeds naturally, Ren returns to sleep after inactivity or explicit dismissal.

## Context From Phase 3
- Full pipeline is live: push-to-talk → STT → LLM → TTS → speaker.
- State machine manages: idle ↔ listening ↔ thinking ↔ speaking.
- In Phase 4, push-to-talk is replaced by wake word detection + VAD. The hotkey remains as a fallback override.
- `StateControls.tsx` (mock debug buttons) must be REMOVED in this phase — the real state machine drives everything now.

## Tasks

### Wake Word Detection
- [ ] Add `porcupine` crate (Picovoice Rust SDK) to Cargo.toml
- [ ] Bundle wake word models as Tauri resources: `hey_ren_en_windows.ppn` and `ren_uyan_en_windows.ppn` (produced via Picovoice Console free tier, ~50KB each)
- [ ] Embed Picovoice AccessKey as a compile-time constant (standard for Porcupine personal use)
- [ ] `WakeWordDetector` in `src-tauri/src/wake/mod.rs`: initialize Porcupine with both `.ppn` models simultaneously; emit `WakeDetected` event on trigger
- [ ] Sleeping state: run wake word detector on continuous 16kHz audio stream; all other modules idle

### State Machine Overhaul
- [ ] Implement full state machine in `src-tauri/src/state/mod.rs`:
  - `Sleeping` → wake word detected → `Waking`
  - `Waking` → play acknowledgment sound + parallel model load → `Listening`
  - `Listening` → VAD end-of-speech → `Thinking` (STT → LLM → TTS)
  - `Thinking` → TTS ready → `Speaking`
  - `Speaking` → playback complete → `Idle`
  - `Idle` → 30s timeout → `Sleeping` (unload models)
  - `Idle` → user speaks → `Listening` (no wake word needed in conversation mode)
  - Any state → dismissal phrase detected → `Sleeping`
- [ ] Parallel model loading on wake: when `Waking` state entered, immediately start loading Whisper and issue LLM keep_alive request — hide latency behind acknowledgment sound + user speaking time
- [ ] Conversation mode timer: reset on each user utterance; expire after configurable duration (default 30s)

### Acknowledgment Sound
- [ ] Small WAV/OGG file bundled as Tauri resource: subtle chime/click that plays on wake
- [ ] Play via `rodio` immediately on wake word detect, before models are ready

### Voice Dismissal
- [ ] Post-process LLM response or STT transcript for dismissal phrases: "goodbye", "sleep", "thanks that's all", "go to sleep" (Turkish equivalents too: "görüşürüz", "tamam yeter")
- [ ] On dismissal phrase: transition to `Sleeping`, unload Whisper from memory

### Model Lifecycle
- [ ] Whisper: load on `Waking`, unload on `Sleeping` (drop the loaded model from memory)
- [ ] LLM: managed by Ollama's `keep_alive` parameter — on wake, issue keep_alive request; on sleep, optionally issue `keep_alive: 0` to unload from VRAM

### Frontend Changes
- [ ] **Remove `StateControls.tsx` and `StateControls.module.css` entirely** — real state machine drives UI now
- [ ] Sleeping state: orb is very dim, barely visible (opacity ~0.3), minimal glow
- [ ] Waking state: dramatic animation — orb brightens rapidly, particles burst outward
- [ ] Update `Orb.module.css` sleeping state to be very subtle
- [ ] Conversation mode indicator: subtle ring or glow difference on orb during `Idle` (conversation mode active vs. deep sleep)

### Hotkey Override
- [ ] Keep `Ctrl+Alt+R` as push-to-talk override (for noisy environments, low wake word confidence)
- [ ] Add hotkey for force-sleep: `Ctrl+Alt+S` (or similar)

## Architecture Notes

### Wake Word Files
- Located at runtime: `<Tauri resource dir>/wake/hey_ren_en_windows.ppn`
- These files are bundled into the binary via `tauri.conf.json` resources field
- The AccessKey string: embed as `const PICOVOICE_ACCESS_KEY: &str = "..."` in `wake/mod.rs` — it is NOT a secret (standard Porcupine personal use pattern)

### State Machine Pattern
The state machine must be the single authority — no other code transitions state directly. All transitions go through `RenStateMachine::transition(to: RenState)` which:
1. Validates the transition is legal
2. Runs exit logic for current state
3. Runs entry logic for next state
4. Emits `ren://state-changed` Tauri event

### Conversation Mode
`Idle` state has an internal timer. When it expires, automatic transition to `Sleeping`. The timer resets on any new user utterance. The timer duration comes from `AppConfig` (default 30s). This is the "follow-up without wake word" feature.

## Acceptance Criteria
- [ ] Saying "Ren" or "Hey Ren" from across the room reliably wakes the assistant
- [ ] Models load in parallel with wake animation so there's no noticeable delay
- [ ] Follow-up questions work without re-triggering wake word
- [ ] Idle timeout returns to sleep after configurable duration
- [ ] Voice dismissal works (English and Turkish phrases)
- [ ] False positive rate on wake word is acceptable
- [ ] StateControls debug UI is gone

## Decisions Made This Phase
<!-- Append here as they happen -->

## Known Risks
- Wake word "Ren" is a short name — high false positive risk on similar sounds in background conversation. Monitor and consider increasing sensitivity threshold or defaulting to "Hey Ren" only.
- Parallel model loading on wake: if Whisper takes longer than the user's speaking time, there will be a gap. Measure and optimize. Whisper medium may be necessary as fallback.
- Picovoice Porcupine free tier: commercial redistribution restrictions. Document clearly. If Ren scales commercially, evaluate openWakeWord as Apache-2.0 licensed alternative.
