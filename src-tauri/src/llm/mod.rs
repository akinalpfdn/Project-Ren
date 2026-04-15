pub mod client;
pub mod conversation;
pub mod ollama_process;
pub mod prompt;

use std::sync::Arc;

use anyhow::Result;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tracing::{info, warn};

use client::{OllamaClient, StreamChunk, ToolCall};
use conversation::Conversation;

use crate::config::defaults::{OLLAMA_KEEP_ALIVE, OLLAMA_MODEL};
use crate::tools::events::{
    ToolExecuting, ToolResultEvent, EVENT_TOOL_EXECUTING, EVENT_TOOL_RESULT,
};
use crate::tools::ToolRegistry;

/// Hard cap on sequential tool calls within a single turn. Protects against
/// runaway loops where the model keeps requesting tools instead of replying.
const MAX_TOOL_ITERATIONS: usize = 4;

/// Drives a full LLM turn: user text in, streamed tokens out.
/// Sends sentence-boundary chunks to `sentence_tx` for the TTS pipeline.
/// Returns the full assembled response text.
pub async fn run_turn(
    app: &AppHandle,
    client: &OllamaClient,
    registry: Arc<ToolRegistry>,
    conversation: &mut Conversation,
    user_text: &str,
    token_tx: &mpsc::Sender<String>,
    sentence_tx: &mpsc::Sender<String>,
) -> Result<String> {
    conversation.push_user(user_text);

    let tools_payload = registry.ollama_tools();
    let tools_ref = if tools_payload.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
        Some(&tools_payload)
    } else {
        None
    };

    let mut iterations = 0usize;

    loop {
        iterations += 1;
        if iterations > MAX_TOOL_ITERATIONS {
            warn!("Tool loop exceeded {} iterations — aborting", MAX_TOOL_ITERATIONS);
            break Ok(String::new());
        }

        let mut sentence_buf = String::new();
        let mut full_text = String::new();
        let mut pending_calls: Vec<ToolCall> = Vec::new();
        let sentence_tx_ref = sentence_tx.clone();
        let token_tx_ref = token_tx.clone();

        let response = client
            .chat_stream(conversation.messages(), tools_ref, |chunk| match chunk {
                StreamChunk::Token(token) => {
                    full_text.push_str(&token);
                    sentence_buf.push_str(&token);
                    let _ = token_tx_ref.try_send(token.clone());

                    if let Some(boundary) = find_sentence_boundary(&sentence_buf) {
                        let sentence = sentence_buf[..boundary].trim().to_string();
                        sentence_buf = sentence_buf[boundary..].to_string();
                        if !sentence.is_empty() {
                            tracing::info!("LLM sentence -> TTS: {:?}", sentence);
                            let _ = sentence_tx_ref.try_send(sentence);
                        }
                    }
                }
                StreamChunk::Done(_) => {
                    let tail = sentence_buf.trim().to_string();
                    if !tail.is_empty() {
                        tracing::info!("LLM sentence -> TTS (tail): {:?}", tail);
                        let _ = sentence_tx_ref.try_send(tail);
                    }
                }
                StreamChunk::ToolCall(call) => {
                    pending_calls.push(call);
                }
            })
            .await?;

        if pending_calls.is_empty() {
            conversation.push_assistant(&response);
            break Ok(response);
        }

        // The model requested tools. Any narrative text produced alongside
        // them is discarded — the model re-generates its final reply after
        // seeing the tool results.
        for call in pending_calls {
            let outcome = execute_tool_call(app, &registry, &call).await;
            conversation.push_tool_result(outcome);
        }
    }
}

async fn execute_tool_call(
    app: &AppHandle,
    registry: &ToolRegistry,
    call: &ToolCall,
) -> String {
    let description = registry
        .get(&call.name)
        .map(|t| t.description().to_string())
        .unwrap_or_default();

    let _ = app.emit(
        EVENT_TOOL_EXECUTING,
        ToolExecuting {
            tool: call.name.clone(),
            description,
        },
    );

    info!("Executing tool '{}' with args {}", call.name, call.arguments);
    let result = registry.dispatch(&call.name, call.arguments.clone()).await;

    match &result {
        Ok(ok) => {
            let _ = app.emit(
                EVENT_TOOL_RESULT,
                ToolResultEvent {
                    tool: call.name.clone(),
                    success: true,
                    summary: ok.summary.clone(),
                },
            );
            format_tool_success(&call.name, ok)
        }
        Err(err) => {
            let message = err.to_string();
            warn!("Tool '{}' failed: {}", call.name, message);
            let _ = app.emit(
                EVENT_TOOL_RESULT,
                ToolResultEvent {
                    tool: call.name.clone(),
                    success: false,
                    summary: message.clone(),
                },
            );
            format_tool_error(&call.name, &message)
        }
    }
}

fn format_tool_success(name: &str, result: &crate::tools::ToolResult) -> String {
    match &result.detail {
        Some(detail) => format!(
            "Tool '{}' succeeded. Summary: {}\nDetails:\n{}",
            name, result.summary, detail
        ),
        None => format!("Tool '{}' succeeded. {}", name, result.summary),
    }
}

fn format_tool_error(name: &str, message: &str) -> String {
    format!("Tool '{}' failed: {}", name, message)
}

/// Returns the byte index of the first sentence boundary in `text`,
/// or `None` if no complete sentence is present yet.
fn find_sentence_boundary(text: &str) -> Option<usize> {
    for (i, ch) in text.char_indices() {
        if matches!(ch, '.' | '?' | '!' | '\n') {
            return Some(i + ch.len_utf8());
        }
    }
    None
}

/// Returns a configured `OllamaClient` using defaults.
pub fn default_client() -> OllamaClient {
    OllamaClient::new(OLLAMA_MODEL, OLLAMA_KEEP_ALIVE)
}
