# Phase 01 — Tauri Shell and Core Orb UI
Status: COMPLETE

## Goal
The app launches, shows a futuristic orb in a small always-on-top window, and animates between mock states.

## What Was Built

### Frontend
- `src/styles/theme.css` — Central design token file. ALL colors, spacings, animation timings, glow values as CSS variables. Single source of truth.
- `src/styles/global.css` — Global resets, font stack, scrollbar styling. Imports theme.css.
- `src/components/Orb.tsx` + `Orb.module.css` — Orb with 6 distinct state animations:
  - `idle` → slow breathing (scale + glow pulse, 3s cycle)
  - `listening` → faster pulse (1.5s), larger orb size
  - `thinking` → 4 orbital particles rotating around the orb
  - `speaking` → 8 waveform bars animating in sequence
  - `waking` → pop-in scale burst animation
  - `error` → red glow + horizontal shake
- `src/components/StateControls.tsx` + `StateControls.module.css` — Temporary debug bar at bottom (Phase 1 only). Shows all state buttons, highlights active. Will be removed in Phase 4 when real state machine is live.
- `src/store/index.ts` — Zustand store. Fields: `currentState`, `error`, `isVisible`. Actions: `setState`, `setError`, `toggleVisibility`, `setVisibility`.
- `src/types/index.ts` — `RenState` union type (8 states), `StateTransition`, `ErrorState` interfaces.
- `src/i18n/index.ts` — react-i18next init, English only, `useSuspense: false`.
- `src/i18n/locales/en.json` — All user-facing strings keyed under: `app`, `tray`, `states`, `errors`, `settings`, `welcome`.
- `src/App.tsx` — Root component. Simulates init with 2s timeout → setState('sleeping'). Renders Orb + StateControls.
- `src/main.tsx` — Entry point. Imports i18n and global.css before App.

### Backend (Rust)
- `src-tauri/src/lib.rs` — System tray with `show_hide` and `quit` items. Left-click on tray icon = toggle. `position_window_bottom_right()` reads monitor size and positions window 20px from right edge, 60px from bottom. Commands: `toggle_window`, `show_window`, `hide_window`.
- `src-tauri/tauri.conf.json` — Window: 300×300, `decorations: false`, `alwaysOnTop: true`, `skipTaskbar: true`, `transparent: true`, `resizable: false`. Tray icon configured.

### Project Files
- `src/styles/theme.css` — Design tokens only. No class rules.
- `.gitignore` — Rust + Node + Tauri patterns.
- `LICENSE` — Apache 2.0, 2026 Ren Contributors.
- `README.md` — Full project overview, stack, features, requirements.
- `CONTRIBUTING.md` — Tool system architecture for contributors, coding standards.

## Styling Approach
CSS Modules (not Tailwind). Tailwind was rejected because:
1. LLM hallucination rate on Tailwind class names is high — arbitrary values and version drift
2. Ren's futuristic glow/particle effects are better expressed in raw CSS
3. CSS variables via `theme.css` provide a clean single-source-of-truth with zero framework overhead

## Decisions Made This Phase
- Apache 2.0 license
- Default TTS voice: `bf_emma`
- Model download: `ollama pull` via child process
- Autostart: opt-in during first-run wizard

## Acceptance Criteria
- [x] Ren.exe launches a window with animated orb
- [x] Window is always on top, does not appear in taskbar or Alt+Tab
- [x] Tray icon present, menu works
- [x] Clicking tray icon toggles window visibility
- [x] Mock state transitions animate smoothly
- [x] No hardcoded colors or strings — all via theme and i18n layers

## Notes for Next Phase
- StateControls component is intentionally kept for Phase 2 testing. Remove in Phase 4.
- `useRenStore.setState` is the mock entry point — in Phase 2 this will be called by Tauri event listeners, not buttons.
- Frontend build confirmed clean: `npm run build` passes with zero errors.
- First `cargo check` takes 20+ minutes on a fresh machine (516 deps). Subsequent builds are cached and fast.
