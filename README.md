# Ren

> **A fully local, voice-first AI assistant for Windows**

Ren is your personal AI assistant that runs entirely on your machine. No cloud inference, no API fees, no data leaves your PC. Wake Ren by voice, speak commands in Turkish, and receive responses in English with a calm, JARVIS-inspired personality.

---

## ✨ Vision

Ren is designed for users who want a personal AI assistant that:

- **Respects privacy** — Everything runs locally. Your conversations never leave your machine.
- **Has no recurring costs** — No API subscriptions, no monthly fees.
- **Feels alive** — An assistant that's "always there" but invisible until called.

The long-term vision is a shippable single-executable product where you download one file, double-click, and Ren takes care of the rest.

---

## 🎯 Current Status

**Phase 1: Core UI** _(Active)_

- [x] Tauri v2 project structure
- [x] React + TypeScript + Framer Motion frontend
- [x] Futuristic orb UI with state-based animations
- [x] System tray integration
- [x] Always-on-top window configuration
- [ ] First public release

---

## 🛠 Tech Stack

- **Framework**: Tauri v2 (Rust backend + React frontend)
- **Languages**: Rust, TypeScript
- **UI**: React with CSS Modules and Framer Motion
- **State Management**: Zustand
- **Localization**: react-i18next (prepared for multi-language support)
- **Target Platform**: Windows 11 (x64) with NVIDIA GPU

---

## 🚀 Planned Features

### MVP (Phases 1–5)

- ✅ Wake word detection ("Ren" or "Hey Ren")
- ✅ Local Turkish speech-to-text (Whisper large-v3)
- ✅ Local LLM reasoning (Qwen 2.5 14B via private Ollama instance)
- ✅ Local English text-to-speech (Kokoro TTS with `bf_emma` voice)
- ✅ Conversation mode (follow-up without re-triggering wake word)
- ✅ Application launcher (Windows apps and Steam games)
- ✅ System controls (volume, brightness, lock, shutdown)
- ✅ Web search (Brave Search API)
- ✅ Weather (Open-Meteo)
- ✅ Calendar (Google Calendar integration)
- ✅ Media controls (Windows Media Transport + Spotify Web API)

### Post-MVP

- File search integration
- Clipboard operations
- Screenshot + OCR
- Reminder/timer system
- Custom wake word training
- Voice cloning for personalized Ren voice
- Vision model integration
- Developer tooling (git status, project switching)
- Auto-updater
- macOS port

---

## 💻 System Requirements

### Minimum

- **OS**: Windows 11 (x64)
- **GPU**: NVIDIA GPU with 12GB VRAM (RTX 4070 Ti or equivalent)
- **RAM**: 16GB
- **Disk**: ~15GB free for models and runtime data

### Recommended

- **RAM**: 32GB+
- **Disk**: SSD for model loading performance

---

## 🏗 Project Structure

```
Project-Ren/
├── src/                    # React frontend
│   ├── components/         # UI components
│   ├── store/             # Zustand state management
│   ├── styles/            # CSS Modules and theme
│   ├── i18n/              # Localization
│   └── types/             # TypeScript definitions
├── src-tauri/             # Rust backend
│   └── src/               # Rust source code
├── DEVPLAN.md             # Development roadmap
├── DECISIONS.md           # Technical decision log
└── CLAUDE.md              # Project instructions for AI assistants
```

---

## 🔧 Development

### Prerequisites

- Node.js (v18+)
- Rust (latest stable)
- npm or pnpm

### Setup

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

---

## 🎨 Design Principles

- **Futuristic aesthetic** — Deep blacks, cyan glows, subtle gradients
- **Centralized theming** — All design tokens in one place
- **Clean code** — SOLID principles, design patterns, no hardcoded values
- **Localization-ready** — All user-facing strings go through i18n layer
- **Privacy-first** — No telemetry, no cloud inference

---

## 📜 License

This project is licensed under the [Apache License 2.0](LICENSE).

---

## 🤝 Contributing

Ren is open source and welcomes contributions. Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on how to add new capabilities, follow the tool system architecture, and maintain code quality.

---

## 🙏 Acknowledgments

Ren builds on the shoulders of giants:

- [Tauri](https://tauri.app) — Cross-platform app framework
- [Ollama](https://ollama.ai) — Local LLM runtime
- [Whisper](https://github.com/openai/whisper) — Speech recognition
- [Kokoro TTS](https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX) — Text-to-speech
- [Porcupine](https://picovoice.ai/platform/porcupine/) — Wake word detection

---

**Built with ❤️ for privacy, performance, and autonomy.**
