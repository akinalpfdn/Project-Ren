# Backlog

Ideas that made the cut during planning but are intentionally not in any active phase yet.
Each entry notes *why* it is on hold so future reviews can judge whether the reason still stands.

---

## Multi-turn background task chains
**Pitched in:** Phase 8 brainstorm (2026-04-14).
**Idea:** Long-running goals that span many turns — "take meeting notes": start a recorder, periodically transcribe, commit markdown to disk. Ren periodically reports progress ("still capturing, 12 minutes in").
**Why deferred:** Compounds on top of memory (8.5), reminders (8.6), STT streaming, file IO, and likely a background process abstraction we do not yet have. Better to build the simpler axes first and return here once the primitives are known to work under real usage.
**Revisit when:** Phase 8.1–8.6 are code-complete and at least 8.5 (memory) has been exercised end-to-end.
