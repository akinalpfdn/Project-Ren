This will be a open source project. You must code accordingly! No messy code, no messy folder hyerarcy no meaningless comments, fully english comments.
The project has to follow coding standarts, design patterns, solid prensibles.
No inline pixels,texts, colors, themes, no default AI gradient design, The UI should feel simple, futuristic and elit.
The project needs to optimized, scalible, maintable, with a well written Readme.md file
## What This Is

Ren is a fully local, voice-first personal assistant for Windows. It runs entirely on the user's machine — no cloud inference, no API fees, no data leaves the PC. Users wake Ren by voice, speak commands in Turkish, and Ren responds in English with a calm, dry, JARVIS-inspired personality. Ren can launch applications and Steam games, control system settings, search the web, manage calendar and weather, control media playback, and more. The long-term vision is a shippable single-executable product where the end user downloads one file, double-clicks, and Ren takes care of the rest.

Ren is designed for users who want a personal AI assistant that respects privacy, has no recurring costs, and feels alive — an assistant that is "always there" but invisible until called.


## Tech Stack
- **Target**: Windows 11 (x64, with NVIDIA GPU). Future ports to macOS possible but out of scope.
- **Framework**: Tauri v2 (Rust backend + React frontend)
- **Backend language**: Rust
- **Frontend language**: TypeScript + React
- **Styling**: Tailwind CSS + Framer Motion (animations)
- **Architecture**: Layered, event-driven. Rust core handles all audio, inference, and OS integration. React frontend handles visualization and settings UI. Communication via Tauri IPC events.
- **Design pattern notes**: State machine pattern for Ren's lifecycle (sleeping / waking / listening / thinking / speaking / idle). Strategy pattern for tool executors (each capability implements a common executor interface). Observer pattern for UI state updates via Tauri event system.
- **Localization**: Prepared for future. Initial release: UI in English only, voice input in Turkish, voice output in English. All user-facing strings in the React frontend must go through an i18n layer (react-i18next) from day one. All Rust-side log messages and error messages should be keyed strings, not hardcoded literals.
- **Theming**: Dark only (futuristic aesthetic — deep blacks, cyan glows, subtle gradients). All colors, typography, spacing, glow intensities, animation timings must live in a central theme file. No hardcoded colors anywhere. The theme file is the single source of truth — if the aesthetic is tuned later, one file changes.

 

## Current Phase
See `.claude/phases/` — always check the active phase file before starting work.

## Rules

### Always active
@rules/common/core.md
@rules/common/decisions.md
@rules/common/git.md
@rules/common/testing.md
@rules/common/debug.md
@rules/common/existing-code.md

### Production readiness (uncomment what applies)
<!-- @rules/common/security.md -->
<!-- @rules/common/deploy.md -->
<!-- @rules/common/observability.md -->
<!-- @rules/common/oss-hygiene.md -->

### UI projects only (remove if backend-only)
@rules/common/frontend.md

### Language rules (uncomment what applies)
<!-- @rules/go/go.md -->
<!-- @rules/swift/swift.md -->
@rules/typescript/typescript.md
<!-- @rules/kotlin/kotlin.md -->
<!-- @rules/flutter/flutter.md -->
@rules/rust/rust.md
<!-- @rules/dotnet/dotnet.md -->
<!-- @rules/python/python.md -->
<!-- @rules/spring/spring.md -->

## Project-Specific Constraints
<!-- Things Claude must not do in this project. Add as you discover them. -->

## Context
<!-- Anything not obvious from the code: target platform, known constraints, current focus. -->
