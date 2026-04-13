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
