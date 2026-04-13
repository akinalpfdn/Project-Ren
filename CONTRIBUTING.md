# Contributing to Ren

Thank you for your interest in contributing to Ren! This document explains the project architecture and guidelines for adding new features.

---

## 📋 Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Project Architecture](#project-architecture)
- [Tool System](#tool-system)
- [Coding Standards](#coding-standards)
- [Pull Request Process](#pull-request-process)

---

## Code of Conduct

Be respectful, inclusive, and constructive. We're building something useful together.

---

## Getting Started

1. **Fork the repository**
2. **Clone your fork**
   ```bash
   git clone https://github.com/yourusername/Project-Ren.git
   cd Project-Ren
   ```
3. **Install dependencies**
   ```bash
   npm install
   cd src-tauri && cargo build
   ```
4. **Run in development mode**
   ```bash
   npm run tauri dev
   ```

---

## Project Architecture

Ren follows a **layered, event-driven architecture**:

### Frontend (React)
- **Components**: UI elements (Orb, StateControls, etc.)
- **Store**: Zustand state management
- **Styles**: CSS Modules with centralized theme (`src/styles/theme.css`)
- **i18n**: Localization layer with react-i18next

### Backend (Rust)
- **Audio Pipeline**: Microphone capture, STT, TTS, playback
- **State Machine**: Ren's lifecycle (sleeping → waking → listening → thinking → speaking)
- **LLM Engine**: Manages private Ollama child process
- **Tool System**: Executors for capabilities (apps, media, web, etc.)

### Key Patterns
- **State Machine Pattern**: Explicit state transitions, no direct mutations
- **Strategy Pattern**: Tool executors implement a common trait
- **Observer Pattern**: React listens to Rust events via Tauri IPC

---

## Tool System

Tools are the capabilities Ren exposes to the LLM. To add a new tool:

### 1. Define the Tool Trait

Each tool implements:
```rust
pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> serde_json::Value;
    fn execute(&self, params: serde_json::Value) -> Result<ToolResult, ToolError>;
}
```

### 2. Create Your Tool

Example: `src-tauri/src/tools/system/volume.rs`
```rust
pub struct VolumeControl;

impl Tool for VolumeControl {
    fn name(&self) -> &str {
        "set_volume"
    }

    fn description(&self) -> &str {
        "Set system volume to a percentage (0-100)"
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "function",
            "function": {
                "name": "set_volume",
                "description": self.description(),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "level": {
                            "type": "number",
                            "description": "Volume level (0-100)"
                        }
                    },
                    "required": ["level"]
                }
            }
        })
    }

    fn execute(&self, params: serde_json::Value) -> Result<ToolResult, ToolError> {
        // Implementation here
    }
}
```

### 3. Register in ToolRegistry

Add your tool to the registry during app initialization:
```rust
registry.register(Box::new(VolumeControl));
```

### Tool Categories

- `system/` — OS-level controls (volume, brightness, lock, shutdown)
- `apps/` — Application launching
- `media/` — Playback controls
- `web/` — Web search and APIs
- `calendar/` — Event management
- `weather/` — Weather queries
- `files/` — File operations
- `meta/` — Ren's own controls

---

## Coding Standards

### General
- **Follow SOLID principles** — especially Single Responsibility and Dependency Inversion
- **Write self-documenting code** — Comments explain "why", never "what"
- **No hardcoded values** — Use the config module or theme file
- **Error handling must be explicit** — No silent catches, no "unknown error" messages

### Rust
- Use `rustfmt` for formatting
- Run `clippy` before committing
- Prefer `Result<T, E>` over panics
- Use structured logging with the `tracing` crate

### TypeScript / React
- Use TypeScript strict mode
- Component props must be typed
- Use CSS Modules for styling — **never inline styles or hardcoded colors**
- All user-facing strings go through `react-i18next`

### Theming
- **All colors, spacing, animations live in `src/styles/theme.css`**
- Never use hardcoded hex colors in components
- Use CSS variables: `var(--ren-accent-primary)`, not `#00d4ff`

### Localization
- All user-facing strings in React go through `i18n` from day one
- Rust-side messages that might be surfaced to UI are returned as i18n keys

---

## Pull Request Process

1. **Create a feature branch**
   ```bash
   git checkout -b feature/my-new-tool
   ```

2. **Make your changes**
   - Follow coding standards
   - Update `DECISIONS.md` if you made a non-trivial technical choice
   - Update `README.md` if you added a user-facing feature

3. **Test your changes**
   ```bash
   npm run tauri dev
   ```

4. **Commit with clear messages**
   ```bash
   git commit -m "Add volume control tool

   - Implement VolumeControl trait
   - Register in ToolRegistry
   - Test on Windows 11"
   ```

5. **Push and create PR**
   ```bash
   git push origin feature/my-new-tool
   ```

6. **PR Guidelines**
   - Describe what you changed and why
   - Reference any related issues
   - Screenshots/videos for UI changes
   - Confirm you tested on Windows 11

---

## Questions?

Open an issue or discussion on GitHub. We're happy to help!

---

**Thank you for contributing to Ren!**
