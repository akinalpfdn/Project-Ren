# Phase 07 — Single-Executable Packaging and First-Run Polish
Status: PENDING (depends on Phase 6)

## Goal
Ren ships as a single portable Windows executable. End user downloads Ren.exe, double-clicks, experiences a polished first-run setup, and is talking to Ren within minutes.

## Context From Phase 6
- All features are complete. This phase is about polish, packaging, and making the end-user experience seamless.
- The download logic from Phases 2 and 3 (Whisper, Ollama binary, Qwen via pull) already exists. In Phase 7 it is unified into a single first-run wizard flow with proper sequencing and a polished UI.
- `AppConfig` and `CredentialStore` are complete. `%APPDATA%\Ren\` directory structure is established.

## Tasks

### First-Run Wizard (Frontend)
- [ ] `FirstRunWizard` component: full-screen overlay that appears when Ren detects missing models on startup
- [ ] Steps rendered in sequence with animated transitions:
  1. **Welcome screen** — Ren logo, "Hello. I am Ren." tagline, futuristic reveal animation
  2. **Hardware check** — GPU detection (name, VRAM), RAM, free disk. Show green/yellow/red status per item. If red (e.g. no CUDA GPU), show warning with "Continue anyway" option (CPU inference will be slow).
  3. **Downloads** — sequential progress bars for each component. Each bar has a label, speed indicator, ETA. Bars animate in as each download starts:
     - Ollama binary (~150MB)
     - Whisper large-v3 (~3GB)
     - Kokoro ONNX (~300MB)
     - Qwen 2.5 14B via `ollama pull` (~9GB)
  4. **Autostart** — "Start Ren automatically with Windows?" toggle with explanation. Default off.
  5. **Ready** — Ren plays welcome message in its own voice ("Hello sir. I am Ren. I'm ready."). Orb visible. "Get started" button transitions to main UI.
- [ ] Wizard state persisted: if user closes Ren mid-download, re-opening detects where it stopped and resumes from that step — check each file for existence + hash
- [ ] All wizard strings through i18n as usual

### Download Manager (Rust - Unified)
- [ ] `FirstRunManager` in `src-tauri/src/setup/first_run.rs`: orchestrates the full download sequence, resumes from last completed step
- [ ] Each step is idempotent: checks if artifact exists and hash matches before downloading
- [ ] Resumable HTTP downloads: use HTTP Range header to resume partial files; partial files stored as `<filename>.part` until complete
- [ ] All hashes pinned in `src-tauri/src/setup/hashes.rs` as constants — never pull from a URL
- [ ] Emit detailed `ren://download-progress` events: `{ step: "whisper" | "ollama_bin" | "kokoro" | "qwen", downloaded_bytes: u64, total_bytes: u64, speed_bps: u64 }`

### Hardware Detection
- [ ] `HardwareChecker` in `src-tauri/src/setup/hardware.rs`:
  - GPU: query via `windows-rs` DXGI or WMI — get adapter name + dedicated VRAM
  - CUDA: check for `nvml.dll` or nvidia-smi presence
  - NVIDIA driver version: from registry `HKLM\SOFTWARE\NVIDIA Corporation\Global\NVDisplay.NvAPI32` or similar
  - RAM: `GlobalMemoryStatusEx` Win32 API
  - Free disk space: `GetDiskFreeSpaceEx` for `%APPDATA%` drive
- [ ] Minimum thresholds as constants (not hardcoded inline):
  - VRAM: 12288 MB
  - RAM: 16384 MB
  - Disk: 15360 MB

### Tauri Build Configuration
- [ ] `tauri.conf.json`: set `bundle.targets` to `["nsis", "msi"]` or just portable exe — evaluate which produces best single-file experience. Portable exe preferred per DEVPLAN.
- [ ] Statically link Whisper and Kokoro runtime into the Rust binary where possible to avoid DLL deps
- [ ] Porcupine `.ppn` files: already bundled as Tauri resources from Phase 4
- [ ] Acknowledgment sound WAV: bundled as Tauri resource from Phase 4
- [ ] App icon: professional icon set. Check if current `icons/` folder has proper sizes (32, 128, 256).
- [ ] Set `productName: "Ren"`, `identifier: "com.ren.assistant"` (already set in Phase 1)

### Uninstall Flow
- [ ] "Uninstall Ren" button in Settings panel (danger zone section)
- [ ] On click: show confirmation dialog with explicit warning: "This will remove all Ren data including downloaded AI models from %APPDATA%\Ren. Your system-wide Ollama installation (if any) will NOT be affected."
- [ ] On confirm: Rust command that:
  1. Terminates child Ollama process
  2. Deletes `%APPDATA%\Ren\` recursively
  3. Removes autostart registry key if set
  4. Writes a small self-delete batch script to `%TEMP%\ren_cleanup.bat`, launches it, exits Ren (batch script waits for Ren to exit then deletes the exe)
- [ ] Self-delete batch script pattern (Windows):
  ```bat
  @echo off
  :loop
  del /f /q "C:\path\to\ren.exe" >nul 2>&1
  if exist "C:\path\to\ren.exe" goto loop
  del "%~f0"
  ```

### Final Polish
- [ ] Logging: ensure `tracing` subscriber writes to `%APPDATA%\Ren\logs\ren.log` with rotation (max 10MB, keep 3 files). Ollama stdout/stderr piped to `%APPDATA%\Ren\logs\ollama.log`.
- [ ] Error messages: audit all `ren://error` emissions — every one has a code and a human-readable message. No bare "Unknown error".
- [ ] README.md: update with final download link, screenshots, full feature list
- [ ] Test against Windows Defender: submit to Windows Defender Intelligence Portal if needed; run multiple scans throughout development
- [ ] Code signing: note as deployment blocker if distributing publicly. Certificate procurement is separate from this task.

## Architecture Notes

### First-Run Detection Logic
```rust
fn needs_first_run(config: &AppConfig) -> bool {
    !whisper_model_path().exists()
    || !ollama_binary_path().exists()
    || !kokoro_model_path().exists()
    // Qwen existence checked via Ollama API after launch
}
```
Each check also verifies SHA256 — a corrupt partial download is treated the same as missing.

### Build Artifacts
- Development: `npm run tauri dev`
- Release: `npm run tauri build` → `src-tauri/target/release/ren.exe`
- The built exe is portable: no installer, no admin required to run.

### File Structure Additions
```
src-tauri/src/
  setup/
    mod.rs
    first_run.rs     — FirstRunManager, step sequencing
    hardware.rs      — HardwareChecker
    hashes.rs        — Pinned SHA256 constants for all downloads
```

## Acceptance Criteria
- [ ] Downloading and running a single Ren.exe is the entire installation process
- [ ] Ren itself never requests admin privileges to launch
- [ ] First-run wizard completes successfully on a clean Windows 11 machine
- [ ] First-run wizard completes on a machine that already has system Ollama (no conflict)
- [ ] Hardware check handles sub-spec machines gracefully with clear messaging
- [ ] Downloads are resumable — closing and reopening mid-download continues where it stopped
- [ ] Uninstall removes Ren completely with one user confirmation, leaves system Ollama untouched
- [ ] Windows Defender does not flag the binary
- [ ] Autostart opt-in works correctly (adds/removes registry key)

## Decisions Made This Phase
<!-- Append here as they happen -->

## Known Risks
- Self-delete on Windows: batch script trick is widely used but can fail if the exe is locked by antivirus. Provide fallback instructions in a "Ren was removed but the exe could not be deleted — you can safely delete it manually" message.
- Binary size: Tauri + Rust + static Whisper/Kokoro runtimes could produce a 200-400MB exe. Test this; Windows Defender heuristics are more aggressive on large unusual binaries.
- Ollama binary download on first run: if Ollama.ai is unreachable (rare but possible), provide a fallback message with manual download instructions.
- Code signing: without a code signing certificate, Windows SmartScreen will show "Unknown publisher" warning. This does NOT block launch but damages trust. Budget for an EV certificate before any public distribution.
