//! Web tools — reach out to public APIs for information Ren cannot derive
//! locally (weather, search). Each tool shares a single `reqwest::Client`
//! so connection reuse and timeouts are consistent.

pub mod search;
pub mod weather;

pub use search::WebSearch;
pub use weather::Weather;

use std::sync::Arc;
use std::time::Duration;

/// Builds the shared HTTP client used by every web tool.
/// A 15-second timeout keeps a hung remote from blocking a voice turn.
pub fn default_client() -> Arc<reqwest::Client> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("Ren/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("failed to build reqwest client");
    Arc::new(client)
}
