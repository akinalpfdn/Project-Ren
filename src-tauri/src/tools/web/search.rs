//! `web.search` — web search via the Brave Search API.
//!
//! Brave was chosen over Bing/Google because it offers a generous free tier
//! (2,000 queries/month) with no credit card, keeping Ren aligned with its
//! zero-recurring-cost ethos. The API key is optional: when missing, the
//! tool returns a `MissingConfig` error that prompts the user to set it in
//! Ren's settings pane.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::tools::{Tool, ToolError, ToolResult};

const BRAVE_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const MAX_RESULTS: usize = 5;

pub struct WebSearch {
    http: Arc<reqwest::Client>,
    api_key: Option<String>,
}

impl WebSearch {
    pub fn new(http: Arc<reqwest::Client>, config: &AppConfig) -> Self {
        Self {
            http,
            api_key: config.brave_api_key.clone(),
        }
    }
}

#[async_trait]
impl Tool for WebSearch {
    fn name(&self) -> &str {
        "web.search"
    }

    fn description(&self) -> &str {
        "Search the public web using Brave Search. Returns a short list of result titles, \
         snippets, and URLs that the assistant can summarise to the user."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query — phrased as the user would type it."
                }
            },
            "required": ["query"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::invalid_args(self.name(), "missing 'query'"))?
            .to_string();

        let api_key = self.api_key.as_deref().ok_or_else(|| ToolError::MissingConfig {
            tool: self.name().into(),
            missing: "Brave Search API key (set 'brave_api_key' in config.json)".into(),
        })?;

        let hits = fetch_results(&self.http, api_key, &query)
            .await
            .map_err(|e| ToolError::execution(self.name(), e))?;

        if hits.is_empty() {
            return Ok(ToolResult::new(format!(
                "No results found for '{}'.",
                query
            )));
        }

        let detail = format_results(&hits);
        let summary = format!(
            "Top {} {} for '{}'.",
            hits.len(),
            if hits.len() == 1 { "result" } else { "results" },
            query,
        );
        Ok(ToolResult::with_detail(summary, detail))
    }
}

#[derive(Debug)]
struct SearchHit {
    title: String,
    snippet: String,
    url: String,
}

async fn fetch_results(
    http: &reqwest::Client,
    api_key: &str,
    query: &str,
) -> Result<Vec<SearchHit>, String> {
    #[derive(Deserialize)]
    struct Response {
        web: Option<WebBlock>,
    }
    #[derive(Deserialize)]
    struct WebBlock {
        results: Option<Vec<WebResult>>,
    }
    #[derive(Deserialize)]
    struct WebResult {
        title: Option<String>,
        description: Option<String>,
        url: Option<String>,
    }

    let resp: Response = http
        .get(BRAVE_ENDPOINT)
        .header("X-Subscription-Token", api_key)
        .header("Accept", "application/json")
        .query(&[
            ("q", query),
            ("count", &MAX_RESULTS.to_string()),
            ("safesearch", "moderate"),
        ])
        .send()
        .await
        .map_err(|e| format!("Brave request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Brave returned error: {}", e))?
        .json()
        .await
        .map_err(|e| format!("could not parse Brave response: {}", e))?;

    let mut hits = Vec::new();
    if let Some(results) = resp.web.and_then(|w| w.results) {
        for r in results.into_iter().take(MAX_RESULTS) {
            let title = r.title.unwrap_or_default();
            let url = r.url.unwrap_or_default();
            if title.is_empty() || url.is_empty() {
                continue;
            }
            hits.push(SearchHit {
                title,
                snippet: strip_html(&r.description.unwrap_or_default()),
                url,
            });
        }
    }
    Ok(hits)
}

fn format_results(hits: &[SearchHit]) -> String {
    let mut out = String::new();
    for (i, hit) in hits.iter().enumerate() {
        out.push_str(&format!("{}. {}\n   {}\n   {}\n", i + 1, hit.title, hit.snippet, hit.url));
    }
    out
}

/// Brave wraps matching terms in `<strong>` — strip any HTML tags so the
/// LLM receives clean text it can quote back to the user.
fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_tags() {
        let stripped = strip_html("a <strong>fast</strong> test");
        assert_eq!(stripped, "a fast test");
    }
}
