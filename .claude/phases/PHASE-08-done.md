# Phase 08 — Assistant Depth
Status: CODE COMPLETE (2026-04-14) — all 6 sub-phases landed; voice-loop acceptance pending physical access.

## Goal
Move Ren from "tool dispatcher" to "assistant that knows context". Ren learns who the user is, notices time, can reason over the clipboard / the filesystem / what's happening on screen, and can proactively nudge when asked.

## Motivation
Every capability in Phases 2–6 is a *request/response primitive*. Phase 8 makes Ren feel continuous: the same name remembered across sessions, "earlier you said X" callbacks, "it's been 20 minutes, still want me to…?" follow-ups. The brainstorm settled on six axes; one (multi-turn background tasks) is deferred to the backlog because it compounds on top of the rest.

---

## Sub-phases (sequenced by dependency and risk)

### 8.1 Temporal awareness — smallest, immediate value
- [ ] `tools/time/mod.rs` with two tools:
  - `time.now` — returns local date, time, weekday, ISO timestamp, timezone offset.
  - `time.sleep_until` (advisory only — reports the duration until a parsed target time so the LLM can talk about it; does not block).
- [ ] System prompt augmented so the LLM sees the current timestamp at the start of every turn.

### 8.2 Clipboard context
- [ ] Global hotkey `Ctrl+Shift+Alt+V` — captures clipboard text and injects it as a contextual preamble for the next user turn.
- [ ] Windows clipboard API via `windows-rs` (`Win32_System_DataExchange` + `Win32_System_Memory`).
- [ ] Frontend: subtle badge on the orb when a clipboard preamble is armed ("context loaded"). Dismissed after the turn or on `Escape`.

### 8.3 File / directory context
- [ ] `tools/files/list_dir` — list a directory (Downloads, Documents, Desktop, arbitrary absolute path), return file names + sizes + modified times.
- [ ] `tools/files/read_text` — read a text file, clamp to a sensible byte budget, return contents for the LLM.
- [ ] `tools/files/summarize_file` — convenience tool: reads then asks the LLM to produce a summary. PDF / DOCX parsing is a later iteration.

### 8.4 System awareness
- [ ] `tools/system/active_window` — Windows `GetForegroundWindow` + process name.
- [ ] `tools/system/resource_usage` — CPU %, RAM %, GPU % (via NVML where available, skip gracefully otherwise).
- [ ] `tools/system/running_apps` — enumerate top-level visible windows with process names.

### 8.5 Memory / personalization
- [ ] User profile file at `%APPDATA%\Ren\memory\profile.md` — editable plain markdown, injected into the system prompt.
- [ ] Conversation archive at `%APPDATA%\Ren\memory\conversations\YYYY-MM-DD.jsonl` — append-only per-day log of user/assistant turns.
- [ ] Memory tools:
  - `memory.remember(fact)` — appends a fact to profile.md under a "Noted" section.
  - `memory.forget(query)` — removes matching lines from profile.md (LLM picks the match).
- [ ] Prompt builder: loads the last N recent archive entries + the current profile on each turn.

### 8.6 Proactive reminders / timers
- [ ] `tools/remind/timer.rs` — ephemeral timers: `timer.start(duration_seconds, label)`, `timer.list`, `timer.cancel(id)`. Fires a `ren://reminder` event on completion, frontend shows a toast and Ren speaks ("Your 10-minute timer for tea is up").
- [ ] `tools/remind/reminder.rs` — persistent reminders: `remind.set(at_iso8601, text)`, `remind.list`, `remind.cancel`. Backed by a JSON file at `%APPDATA%\Ren\memory\reminders.json`.
- [ ] State observer task polling the reminder store once a minute; fires events when the wall-clock hits.
- [ ] Escapes: any active timer or reminder is surfaced in the Orb as a faint ring; tray menu shows the next-due one.

---

## Architecture Notes

### File structure additions
```
src-tauri/src/
  tools/
    time/
      mod.rs
    files/
      list_dir.rs
      read_text.rs
      summarize_file.rs
    system/
      active_window.rs
      resource_usage.rs
      running_apps.rs
    remind/
      mod.rs
      timer.rs
      reminder.rs
  memory/
    mod.rs          — profile + conversation archive IO
```

### New events
- `ren://reminder` — payload: `{ id, label, kind: "timer" | "reminder" }`.
- `ren://clipboard-armed` — payload: `{ preview: string }` (first 80 chars, truncated).

### New config keys
- `memory_enabled: bool` (default true).
- `memory_archive_retention_days: u32` (default 30).

## Acceptance Criteria
- [ ] "Ren, saat kaç" — reports local time with timezone.
- [ ] `Ctrl+Shift+Alt+V` with clipboard text, then "bunu Türkçeye çevir" — Ren translates.
- [ ] "Ren, masaüstündeki en son dosyayı aç ve özet geç" — list_dir + summarize_file chain.
- [ ] "Ren, şu an ne yapıyorum" — reports active window title.
- [ ] "Ren, adımı hatırla — Akın" then restart — next session Ren greets by name.
- [ ] "Ren, 10 dakika sonra bana hatırlat çay demle" — 10 minutes later Ren speaks the reminder.

## Decisions Made This Phase
<!-- Append here as they happen -->

## Known Risks
- **Clipboard context leakage** — sensitive content (passwords, credit card numbers) must never be archived or sent to third-party tools. Always stays local to the LLM; never enters the conversation archive unless the user explicitly asks.
- **Reminder scheduling accuracy** — a once-per-minute poll is enough for human-scale reminders, not for precision timing. Document this.
- **Profile drift** — `memory.remember` / `memory.forget` are LLM-driven; bad calls could corrupt the profile. Add an automatic rolling backup (`profile.md.bak`) on every write.
- **NVML availability** — not every system has NVML at a stable path. Resource-usage tool must degrade gracefully without it.
