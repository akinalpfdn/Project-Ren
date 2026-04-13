# Decisions

This file tracks all non-trivial technical decisions made during this project.
See `rules/common/decisions.md` for the logging format and rules.

---

## 2026-04-13 — License Selection
**Chosen:** Apache 2.0
**Alternatives:** MIT License
**Why:** Apache 2.0 provides explicit patent grant protection while still being permissive for commercial use. Given that Ren is a complex project with multiple dependencies and potential for forks, the additional legal clarity around patent rights is valuable.
**Trade-offs:** Slightly more verbose than MIT, requires attribution in NOTICE file. MIT would have been simpler but offers less protection.
**Revisit if:** Community feedback strongly favors MIT, or if the patent protection clause causes friction with certain integrations.

---

## 2026-04-13 — Autostart Configuration UX
**Chosen:** Opt-in during first-run setup wizard
**Alternatives:** Only accessible from settings panel post-install
**Why:** Voice assistants are expected to be "always there" — users naturally want them to start with Windows. Offering this during first run when the user is already engaged improves UX and sets proper expectations. Still requires explicit consent.
**Trade-offs:** Adds one more step to first-run wizard. Alternative would keep first-run minimal but force users to hunt through settings for this common feature.
**Revisit if:** First-run completion rate drops significantly, or users report feeling pressured by the prompt.

---

## 2026-04-13 — Qwen Model Download Method
**Chosen:** Use `ollama pull` command against Ren's child Ollama instance
**Alternatives:** Download GGUF directly from Hugging Face and construct Ollama manifest manually
**Why:** Leverages Ollama's built-in download resumption, integrity checks, and model registry. Simpler, less code to maintain, fewer edge cases. Ollama already handles SHA verification and retry logic.
**Trade-offs:** Depends on Ollama's infrastructure availability during user's first run. Direct HF download would give more control but requires implementing resumption and manifest generation ourselves.
**Revisit if:** Ollama registry experiences significant outages, or if we need to support airgapped installations where internet access is restricted.

---

## 2026-04-13 — Default TTS Voice Selection
**Chosen:** `bf_emma` (British Female, calm and elegant)
**Alternatives:** `af_bella` (American Female, warm), or custom blend like `bf_emma:0.7,af_bella:0.3`
**Why:** JARVIS-inspired personality calls for calm, dry, authoritative tone. British accent naturally conveys this elegance and composure. `bf_emma` aligns perfectly with "addresses user as 'sir'" and the futuristic aesthetic.
**Trade-offs:** May feel less relatable to American users compared to `af_bella`. Custom blend could offer unique character but requires testing to find balance and adds complexity.
**Revisit if:** User testing shows strong preference for American accent, or if `bf_emma` quality is insufficient for Ren's response patterns.

---

## 2026-04-13 — Native ML Libraries Behind Cargo Feature Flags
**Chosen:** `whisper-rs`, `ort`, `pv_porcupine` are all optional and gated behind `stt`, `tts`, `wake` features. Default build links zero native ML libraries.
**Alternatives:** Require all native libraries on every build machine, or ship prebuilt DLLs with the crate.
**Why:** Contributors without a C++ toolchain, CMake, ONNX Runtime, or a Picovoice access key must still be able to run `cargo check` and `cargo build` on a fresh clone. Feature flags give a clean "compile path" for CI and contributors while the installed binary enables all three features.
**Trade-offs:** Three stubs to maintain (one per feature-gated module); `#[cfg(feature = "…")]` sprinkled through engine modules; slightly higher cognitive load when reading the code.
**Revisit if:** A pure-Rust alternative becomes competitive for any of the three (e.g. `openwakeword-rs` for wake), eliminating the native dependency entirely.

---

## 2026-04-13 — `pv_porcupine` via Git Dependency
**Chosen:** Pull `pv_porcupine` from `github.com/Picovoice/porcupine` at tag `v3.0`, not from crates.io.
**Alternatives:** Pin a crates.io version, publish a fork to crates.io, write a custom Porcupine FFI binding.
**Why:** All published versions of `pv_porcupine` on crates.io (3.0.0 through 3.0.3) are yanked. The crate itself appears abandoned on the registry. Picovoice's official GitHub repository still hosts the same Rust binding under `binding/rust/` at a stable `v3.0` tag.
**Trade-offs:** Git dependencies do not participate in Cargo's version resolution the same way as registry deps; `cargo publish` is blocked while a git dep exists; contributors need network access to GitHub on first build. Writing a custom FFI binding would remove the dependency but duplicates maintained upstream code.
**Revisit if:** Picovoice republishes `pv_porcupine` to crates.io, we switch to `openwakeword-rs` (pure Rust), or we want to publish Ren itself to crates.io.

---

## 2026-04-13 — `ort` Version Pinned to Exact Prerelease
**Chosen:** `ort = "=2.0.0-rc.10"`.
**Alternatives:** Track the `2` semver range, let Cargo pick the newest rc.
**Why:** `ort 2.0.0-rc.*` versions are not API-compatible with each other. `voice_activity_detector 0.2.x` also pins this exact version. A floating range would cause mixed rc versions in the dependency tree.
**Trade-offs:** Manual bumps required whenever `voice_activity_detector` or `ort` ships a breaking prerelease. No automatic security patches on the crate.
**Revisit if:** `ort` reaches a stable `2.0.0` release, or VAD drops its exact pin.

---

## 2026-04-13 — Async Mutex for LLM/STT/TTS State
**Chosen:** `tokio::sync::Mutex` for `Conversation`, `WhisperEngine`, `KokoroEngine` (held across `.await`).
**Alternatives:** Keep `std::sync::Mutex` and aggressively scope guards with extra cloning or message-passing.
**Why:** These types are consumed exclusively from async tasks, and their operations (`load`, `transcribe`, `synthesize`, streamed `run_turn`) are themselves async. Holding a `std::sync::MutexGuard` across `.await` makes the future `!Send`, which prevents `tokio::spawn`. `tokio::sync::Mutex` is built for this case — its guard is `Send`.
**Trade-offs:** Slightly higher overhead than a spinlock; `.await` must be inserted at lock acquisition; care required not to deadlock across yield points. Accepted because the contention is effectively single-writer per engine.
**Revisit if:** Benchmarks show the async lock becomes a hotspot, or if we refactor engines to an actor/channel model.

---

## 2026-04-13 — Audio Output on a Dedicated OS Thread
**Chosen:** `AudioPlayer` spawns a dedicated `std::thread` that owns `rodio::OutputStream`; the public handle is an `mpsc::UnboundedSender<PlayCommand>`.
**Alternatives:** Use `tokio::task::spawn_blocking` around each playback call; force-wrap the stream in `unsafe impl Send`.
**Why:** `rodio::OutputStream` (via CPAL) is `!Send`. Storing it in a shared `Arc<AudioPlayer>` and capturing it in a `tokio::spawn` future violates the `Send` bound. A dedicated thread owns the stream for its entire lifetime; async tasks send `PlayCommand`s and await a `oneshot::Receiver<Result<()>>` for completion. Safe, no `unsafe`.
**Trade-offs:** One extra OS thread at idle (~few KB stack). Shutdown must coordinate the thread (currently it exits when the command channel closes).
**Revisit if:** We need multiple concurrent playback streams, or if CPAL ships a `Send` variant.

---

## 2026-04-13 — State Observer Pattern via `tokio::sync::broadcast`
**Chosen:** `RenStateMachine` owns a `broadcast::Sender<RenState>`; `subscribe()` exposes receivers for in-process observers. `lib.rs` spawns long-lived observer tasks (conversation timer, model unloader) that react to state transitions.
**Alternatives:** Call side-effect code directly inside `RenStateMachine::transition`; use `tauri::Emitter` events for both UI and backend observers.
**Why:** Keeps `RenStateMachine` free of `tokio::time`, model references, and other side-effect-heavy concerns — easy to unit-test in isolation. New observers are one `spawn` away. Tauri events stay as the UI boundary; `broadcast` is the in-process boundary.
**Trade-offs:** Broadcast senders drop lagging receivers; observers must handle `RecvError::Lagged`. Slight risk of fan-out bugs if too many observers subscribe.
**Revisit if:** We need ordered / transactional observation (broadcast is fire-and-forget), or if the observer count grows beyond a handful.

---

## 2026-04-13 — Dismissal Phrase Detector as a Pure Rust Module
**Chosen:** `dismissal::is_dismissal` — case-insensitive substring match against a fixed list of English and Turkish phrases.
**Alternatives:** Ask the LLM "is this a goodbye?", use a small classifier, regex-based matcher.
**Why:** Dismissal must be instant, deterministic, and run before the LLM is even consulted. Substring match against ~12 phrases is O(n*m) with n and m both tiny; latency is effectively zero. No model to load, no API to call. Unit-testable from a single file.
**Trade-offs:** Will not catch paraphrases like "alright I'm done with you now". Acceptable — users can learn the magic words, and the hotkey `Ctrl+Alt+S` remains as a fallback.
**Revisit if:** User research shows frustration with the fixed list, or if we add enough languages that maintaining the list becomes unwieldy.

---

## 2026-04-13 — Tool Capability Model (Strategy Pattern + Registry)
**Chosen:** Every capability implements a `Tool` trait (name, description, JSON Schema parameters, safety level, async execute). A `ToolRegistry` owns `Arc<dyn Tool>` instances, exposes `ollama_tools()` in Ollama's function-calling shape, and dispatches calls by name.
**Alternatives:** Match on tool name inside a single large function; generate the tool list at compile time via a macro; let each subsystem register its own IPC commands.
**Why:** The strategy pattern keeps each capability self-contained — its JSON Schema, executor, and safety classification live in one file. Adding a new tool is a two-line registration; no central match to grow. Registry-based dispatch gives a single point to emit `ren://tool-executing` / `ren://tool-result` events and enforce confirmation on destructive tools.
**Trade-offs:** One small heap allocation per tool via `Arc<dyn Tool>`; runtime dispatch through a vtable instead of static calls. Both are negligible at tool-call latency.
**Revisit if:** The set of tools grows large enough that schema size in the system prompt hurts token budget, or we need per-user permission gating that the single registry can't express.

---

## 2026-04-13 — App and Steam Launchers Use Fuzzy Matching (Jaro-Winkler)
**Chosen:** Lowercase the query and each candidate, compute Jaro-Winkler similarity, take the highest score that clears a threshold (0.82 for Start Menu apps, 0.80 for Steam games). A small hand-maintained alias table (`chrome` → `Google Chrome`) short-circuits the common cases.
**Alternatives:** Exact substring match; Levenshtein distance; embed + cosine similarity via a small local model.
**Why:** Voice STT for English app names in a Turkish speaker's pipeline produces systematic misspellings ("spotfy", "vscode", "counter strike two"). Jaro-Winkler favours prefix agreement, which matches how users truncate names. The alias table handles the ~15 titles everyone calls by something other than their Start Menu entry.
**Trade-offs:** Threshold is empirical; tuning is needed when users report false hits ("chrome" → "Chrome Remote Desktop"). Embeddings would give semantic matching but cost load time and model weight.
**Revisit if:** False-match rate from real usage exceeds ~5%, or the alias list balloons past ~50 entries.

---

## 2026-04-13 — Steam Library Discovery via VDF Parser
**Chosen:** Locate Steam via `HKCU\Software\Valve\Steam\SteamPath` (falling back to default install paths). Parse `steamapps/libraryfolders.vdf` with an in-tree recursive-descent parser to find additional library roots, then read each `appmanifest_*.acf` for `appid` + `name`. Cache results to `%APPDATA%\Ren\cache\steam_games.json`.
**Alternatives:** Add a crate like `keyvalues-parser` or `steamlocate`; scrape the Steam web API for the user's owned games; require the user to manually list game paths.
**Why:** The VDF text format we need is simple enough (key/value pairs, brace-delimited objects, `//` comments) that a purpose-built parser is under 150 lines and has zero runtime cost beyond what `serde_json` would have. A crate would add a dependency, a trust boundary, and usually more features than we need. Only Steam-installed games work this way — scraping the web API would need auth and wouldn't know which games are installed locally.
**Trade-offs:** The parser handles only what Valve's current format emits; malformed manifests get skipped with a warning rather than propagated. If Valve changes the format we must update the parser.
**Revisit if:** Valve ships a new manifest format, or we need to cover launchers beyond Steam (Epic, GOG) and a common parser crate starts making sense.

---

## 2026-04-13 — Files Tool Opens an Allow-listed Set of Standard Folders
**Chosen:** `files.open_folder` accepts a `folder` argument restricted to a fixed enum (`downloads`, `documents`, `desktop`, `pictures`, `music`, `videos`) resolved under `%USERPROFILE%`. Arbitrary paths are not accepted.
**Alternatives:** Accept any absolute path; accept an arbitrary "folder name" and search the disk; open files as well as folders.
**Why:** The tool runs whatever the LLM hands it through `explorer.exe`. Accepting arbitrary paths turns a voice tool into an arbitrary-path-opener — fine for a trusted local LLM today, but a footgun if prompt injection ever reaches the LLM via search results or email. An enum is trivially safe and covers every request a user is likely to voice.
**Trade-offs:** Users cannot open custom folders through voice. They can still open them via the apps launcher (`start menu` entries) or a shortcut.
**Revisit if:** Users repeatedly ask for folders outside the allow list, at which point we can extend the enum explicitly rather than widening the contract.

---

## 2026-04-13 — Weather via Open-Meteo, Search via Brave
**Chosen:** `web.weather` hits Open-Meteo (no key required) for both geocoding and current forecast. `web.search` uses the Brave Search API with a user-supplied key; missing key returns a typed `MissingConfig` error surfaced in the UI.
**Alternatives:** OpenWeatherMap (requires key, has a free tier); Google/Bing (paid, credit card); DuckDuckGo (no structured API).
**Why:** Open-Meteo aligns perfectly with Ren's "no recurring cost" ethos — anonymous, rate-limited only by fair-use. Brave has the most generous free search tier (2,000 queries/month, no credit card) and a stable JSON API. Keeping the key optional means the product still ships usefully if the user never configures search.
**Trade-offs:** Brave's index is smaller than Google's; some niche queries miss. Open-Meteo's geocoder occasionally returns surprising "same-name city, different country" hits for ambiguous place names.
**Revisit if:** Brave's free tier shrinks, or user feedback shows search quality is blocking common voice queries.

---

## 2026-04-13 — Shared `reqwest::Client` for Web Tools
**Chosen:** A single `Arc<reqwest::Client>` is built once at startup (15-second timeout, Ren user-agent) and shared across every web tool.
**Alternatives:** Each tool builds its own client; a new client per request.
**Why:** `reqwest::Client` pools TCP and TLS connections; sharing one client across tools means back-to-back voice turns reuse warm HTTP/2 streams to the same hosts. Building per-request throws away that pool and adds handshake latency that is audible in voice UX.
**Trade-offs:** A tool that wants per-tool proxying or different timeout behaviour cannot opt out without refactor. None of the current tools need that.
**Revisit if:** We add a tool with fundamentally different network semantics (long-poll, WebSocket, host-specific proxy).

---

## 2026-04-14 — Bump `whisper-rs` 0.13 → 0.16
**Chosen:** Upgraded `whisper-rs` from `0.13` to `0.16` (pulls `whisper-rs-sys` 0.13.1).
**Alternatives:** Pin `whisper-rs-sys` to an older compatible point release; vendor whisper.cpp at a pinned commit.
**Why:** `whisper-rs` 0.13.2 with `whisper-rs-sys` 0.11.1 emits an opaque `whisper_full_params` (`_address: u8`, size 264) under `bindgen` 0.69.5 on MSVC, producing 71 "unknown field" errors. 0.14.x moved to a size-assertion failure against the same bindgen output. 0.16 tracks a newer whisper.cpp release where the struct is fully representable by bindgen, compiling cleanly. Also unblocks the `cuda` feature which is mandatory for sub-second Turkish transcription on the target RTX-class hardware.
**Trade-offs:** Minor API migration (`full_n_segments` loses its `Result`, segment text reads via `WhisperState::get_segment` + `WhisperSegment::to_str`) in `stt/whisper.rs`.
**Revisit if:** A future whisper.cpp / `whisper-rs` release regresses opaque-struct handling or changes the `FullParams` API again.

---

## 2026-04-14 — Hotkeys: Ctrl+Alt+{R,S} → Ctrl+Shift+Alt+{R,S}
**Chosen:** Push-to-talk is `Ctrl+Shift+Alt+R`; force-sleep is `Ctrl+Shift+Alt+S`.
**Alternatives:** Keep the dual-modifier binding; fall back to alternate keys (`J`/`K`, function keys).
**Why:** `Ctrl+Alt+R` collides with browser reload and several productivity tools, producing "HotKey already registered" errors at `GlobalHotKeyManager::register`. A triple-modifier chord is practically never bound by another application while remaining pronounceable.
**Trade-offs:** Slightly harder to press single-handed; acceptable because the wake word is the primary trigger and the hotkey is only a fallback override.
**Revisit if:** We observe user complaints about chord ergonomics or move PTT into a dedicated hardware button / tray menu.

---

## 2026-04-14 — `app_data_dir` uses `BaseDirs` + single `Ren` segment
**Chosen:** Derive the data directory from `directories::BaseDirs::data_dir()` and append `Ren`, producing `%APPDATA%\Ren\` on Windows.
**Alternatives:** Keep `ProjectDirs::from("com", "ren", "Ren")` which yields the XDG-style `%APPDATA%\ren\Ren\data\`.
**Why:** Documentation across `CLAUDE.md`, `DEVPLAN.md`, and every phase file states `%APPDATA%\Ren\...` as the canonical layout. The `ProjectDirs` output diverged silently, breaking manual model placement and confusing first-run setup. A single-segment folder also matches Windows convention for native apps.
**Trade-offs:** We lose cross-platform XDG compliance (irrelevant — Ren is Windows-only per stack decision); existing users (if any) must migrate manually.
**Revisit if:** Ren ever targets macOS or Linux first-class, at which point a platform-specific resolver becomes necessary.
