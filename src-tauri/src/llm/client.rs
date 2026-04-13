use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::llm::ollama_process::active_port;

/// A single message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
    pub fn tool_result(content: impl Into<String>) -> Self {
        Self { role: "tool".into(), content: content.into() }
    }
}

/// Tool call returned by the LLM in a streaming response.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// A single streamed chunk from the Ollama API.
#[derive(Debug)]
pub enum StreamChunk {
    /// A text token delta.
    Token(String),
    /// The model wants to call a tool.
    ToolCall(ToolCall),
    /// Stream finished — includes final assembled text.
    Done(String),
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
    keep_alive: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a serde_json::Value>,
}

#[derive(Deserialize)]
struct ChatResponseChunk {
    message: Option<ChunkMessage>,
    done: Option<bool>,
}

#[derive(Deserialize)]
struct ChunkMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Deserialize)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Deserialize)]
struct OllamaToolCallFunction {
    name: String,
    arguments: serde_json::Value,
}

/// HTTP wrapper around Ollama's `/api/chat` endpoint.
pub struct OllamaClient {
    http: Client,
    model: String,
    keep_alive: String,
}

impl OllamaClient {
    pub fn new(model: impl Into<String>, keep_alive: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            model: model.into(),
            keep_alive: keep_alive.into(),
        }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", active_port())
    }

    /// Send a streaming chat request.
    /// Calls `on_chunk` for each streamed chunk.
    pub async fn chat_stream(
        &self,
        messages: &[Message],
        tools: Option<&serde_json::Value>,
        mut on_chunk: impl FnMut(StreamChunk),
    ) -> Result<String> {
        let url = format!("{}/api/chat", self.base_url());

        let body = ChatRequest {
            model: &self.model,
            messages,
            stream: true,
            keep_alive: &self.keep_alive,
            tools,
        };

        let response = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to reach Ollama — is the child process running?")?;

        if !response.status().is_success() {
            anyhow::bail!("Ollama returned HTTP {}", response.status());
        }

        let mut stream = response.bytes_stream();
        let mut full_text = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("Stream read error")?;
            let line = String::from_utf8_lossy(&bytes);

            for raw in line.lines() {
                if raw.is_empty() {
                    continue;
                }
                let parsed: ChatResponseChunk = match serde_json::from_str(raw) {
                    Ok(v) => v,
                    Err(e) => {
                        debug!("Skipping unparseable chunk: {} — {}", raw, e);
                        continue;
                    }
                };

                if let Some(msg) = parsed.message {
                    // Tool calls
                    if let Some(calls) = msg.tool_calls {
                        for call in calls {
                            on_chunk(StreamChunk::ToolCall(ToolCall {
                                name: call.function.name,
                                arguments: call.function.arguments,
                            }));
                        }
                    }
                    // Token delta
                    if let Some(token) = msg.content {
                        if !token.is_empty() {
                            full_text.push_str(&token);
                            on_chunk(StreamChunk::Token(token));
                        }
                    }
                }

                if parsed.done.unwrap_or(false) {
                    on_chunk(StreamChunk::Done(full_text.clone()));
                }
            }
        }

        Ok(full_text)
    }

    /// Issue a keep_alive ping to keep the model warm in VRAM.
    pub async fn ping(&self) -> Result<()> {
        let url = format!("{}/api/chat", self.base_url());
        let body = ChatRequest {
            model: &self.model,
            messages: &[Message::user("")],
            stream: false,
            keep_alive: &self.keep_alive,
            tools: None,
        };
        self.http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Keep-alive ping failed")?;
        Ok(())
    }
}
