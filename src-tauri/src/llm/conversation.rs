use crate::llm::{client::Message, prompt::build_system_prompt};
use crate::tools::ToolRegistry;

/// In-memory conversation history for the current session.
/// Resets on Ren restart. The system prompt is always at index 0.
pub struct Conversation {
    messages: Vec<Message>,
}

impl Conversation {
    /// Build a fresh conversation. Pass `Some(registry)` to inject the
    /// available-tools block into the system prompt.
    pub fn new(registry: Option<&ToolRegistry>) -> Self {
        Self {
            messages: vec![Message::system(build_system_prompt(registry))],
        }
    }

    /// Append a user message.
    pub fn push_user(&mut self, text: impl Into<String>) {
        self.messages.push(Message::user(text));
    }

    /// Append an assistant response.
    pub fn push_assistant(&mut self, text: impl Into<String>) {
        self.messages.push(Message::assistant(text));
    }

    /// Append a tool result (used in Phase 5 tool call pipeline).
    pub fn push_tool_result(&mut self, result: impl Into<String>) {
        self.messages.push(Message::tool_result(result));
    }

    /// Full message slice — passed directly to OllamaClient.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Reset to a fresh conversation (keeps system prompt).
    pub fn reset(&mut self) {
        self.messages.truncate(1);
    }
}
