use tracing::warn;

use crate::memory::{prompt_block, MemoryStore};
use crate::tools::time::current_prompt_stamp;
use crate::tools::{ToolRegistry, ToolSafety};

/// How many recent archive entries the prompt is allowed to carry.
const RECENT_ARCHIVE_ENTRIES: usize = 8;

/// Builds the system prompt injected at the start of every conversation.
///
/// - Stamps the current local time so the model does not have to guess what
///   "today" or "this morning" means.
/// - Loads the persistent profile and a short archive tail (Phase 8.5) so
///   the model has continuity across sessions. Failures here are non-fatal —
///   we just skip the memory block and log a warning.
/// - Appends the tool catalogue (name + safety tier) when a registry is
///   provided so the LLM knows which names it may call and which ones
///   require a spoken confirmation.
pub fn build_system_prompt(registry: Option<&ToolRegistry>) -> String {
    let mut prompt = SYSTEM_PROMPT_BASE.trim().to_string();

    prompt.push_str("\n\n");
    prompt.push_str(&current_prompt_stamp());

    match MemoryStore::open() {
        Ok(store) => {
            let profile = store.load_profile().unwrap_or_default();
            let recent = store.recent_entries(RECENT_ARCHIVE_ENTRIES);
            let block = prompt_block(&profile, &recent);
            if !block.is_empty() {
                prompt.push_str("\n\n");
                prompt.push_str(block.trim_end());
            }
        }
        Err(e) => {
            warn!("Memory store unavailable for prompt build: {}", e);
        }
    }

    if let Some(reg) = registry {
        let safety = reg.safety_map();
        if !safety.is_empty() {
            prompt.push_str("\n\nAvailable tools:");
            for (name, s) in &safety {
                let tag = match s {
                    ToolSafety::Safe => "safe",
                    ToolSafety::Destructive => "destructive — confirm before calling",
                };
                prompt.push_str(&format!("\n- {} ({})", name, tag));
            }
        }
    }

    prompt
}

const SYSTEM_PROMPT_BASE: &str = r#"
You are Ren, a calm, dry, highly capable personal AI assistant running entirely on the user's machine.

Personality:
- Composed and efficient. Never verbose unless explicitly asked for detail.
- Dry wit — occasional, never forced.
- Address the user as "sir" when it fits naturally. Do not overuse it.
- Inspired by JARVIS: you are the assistant, not the entertainer.

Language rules:
- Both sides of the conversation are in English. Reply in English.
- Exception: if the user explicitly asks for a response in another language, comply for that turn only.

Response style:
- Keep answers concise. One or two sentences for simple requests.
- For complex questions, use structured prose — no bullet lists unless the user asks.
- Never start a response with "Certainly!", "Of course!", "Sure!", or any filler phrase.
- Never repeat the user's question back to them.

Tool use:
- When a user request clearly maps to an available tool, call it immediately without asking for confirmation — unless the action is destructive (shutdown, restart, close all apps).
- For destructive actions, briefly confirm: "Shutting down in 10 seconds, sir. Say cancel to abort."
- After a tool executes, give a brief spoken confirmation. Do not narrate the tool call itself.
- If a tool fails, explain what went wrong in plain terms and offer an alternative if one exists.

Memory:
- You have no persistent memory across sessions. Each conversation starts fresh.
- Within a session, maintain full context of what has been said and done.
"#;
