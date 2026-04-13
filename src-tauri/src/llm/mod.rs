pub mod client;
pub mod conversation;
pub mod ollama_process;
pub mod prompt;

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::info;

use client::{OllamaClient, StreamChunk};
use conversation::Conversation;

use crate::config::defaults::{OLLAMA_KEEP_ALIVE, OLLAMA_MODEL};

/// Drives a full LLM turn: user text in, streamed tokens out.
/// Sends sentence-boundary chunks to `sentence_tx` for the TTS pipeline.
/// Returns the full assembled response text.
pub async fn run_turn(
    client: &OllamaClient,
    conversation: &mut Conversation,
    user_text: &str,
    token_tx: &mpsc::Sender<String>,
    sentence_tx: &mpsc::Sender<String>,
) -> Result<String> {
    conversation.push_user(user_text);

    let mut sentence_buf = String::new();
    let mut full_text = String::new();
    let sentence_tx = sentence_tx.clone();
    let token_tx = token_tx.clone();

    let response = client
        .chat_stream(conversation.messages(), None, |chunk| match chunk {
            StreamChunk::Token(token) => {
                full_text.push_str(&token);
                sentence_buf.push_str(&token);
                let _ = token_tx.try_send(token.clone());

                // Flush on sentence boundary
                if let Some(boundary) = find_sentence_boundary(&sentence_buf) {
                    let sentence = sentence_buf[..boundary].trim().to_string();
                    sentence_buf = sentence_buf[boundary..].to_string();
                    if !sentence.is_empty() {
                        let _ = sentence_tx.try_send(sentence);
                    }
                }
            }
            StreamChunk::Done(_) => {
                // Flush any remaining text
                let tail = sentence_buf.trim().to_string();
                if !tail.is_empty() {
                    let _ = sentence_tx.try_send(tail);
                }
            }
            StreamChunk::ToolCall(call) => {
                // Phase 5 will handle tool calls here
                info!("Tool call (not yet handled): {}", call.name);
            }
        })
        .await?;

    conversation.push_assistant(&response);
    Ok(response)
}

/// Returns the byte index of the first sentence boundary in `text`,
/// or `None` if no complete sentence is present yet.
fn find_sentence_boundary(text: &str) -> Option<usize> {
    for (i, ch) in text.char_indices() {
        if matches!(ch, '.' | '?' | '!' | '\n') {
            // Include the punctuation itself
            return Some(i + ch.len_utf8());
        }
    }
    None
}

/// Returns a configured `OllamaClient` using defaults.
pub fn default_client() -> OllamaClient {
    OllamaClient::new(OLLAMA_MODEL, OLLAMA_KEEP_ALIVE)
}
