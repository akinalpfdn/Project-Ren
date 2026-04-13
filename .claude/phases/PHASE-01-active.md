# Phase 01 — Tauri Shell and Core Orb UI
Status: ACTIVE

## Goal
The app launches, shows a futuristic orb in a small always-on-top window, and animates between mock states.

## Tasks
- [ ] Scaffold Tauri v2 project with React + TypeScript + Tailwind + Framer Motion
- [ ] Create centralized theme system (colors, spacing, animations)
- [ ] Set up i18n layer with react-i18next (English only for now)
- [ ] Build orb component with breathing idle animation
- [ ] Implement mock state machine (idle → listening → thinking → speaking)
- [ ] Create distinct visuals for each state (breathing, pulse, particles, waveform)
- [ ] Configure frameless, always-on-top, transparent window at bottom-right
- [ ] Implement system tray with Show/Hide and Quit options
- [ ] Add Apache 2.0 LICENSE and README.md

## Acceptance Criteria
- [ ] Ren.exe launches a window with animated orb
- [ ] Window is always on top, does not appear in taskbar or Alt+Tab
- [ ] Tray icon present, menu works
- [ ] Clicking tray icon toggles window visibility
- [ ] Mock state transitions animate smoothly
- [ ] No hardcoded colors or strings — all via theme and i18n layers

## Decisions Made This Phase
- 2026-04-13: Apache 2.0 license selected
- 2026-04-13: Autostart opt-in during first run (not just settings)
- 2026-04-13: Use `ollama pull` for Qwen model download
- 2026-04-13: Default TTS voice is `bf_emma` (British Female, calm/elegant)

---
**Note:** No code snippets in phase files. Tasks are goals, not implementations.
