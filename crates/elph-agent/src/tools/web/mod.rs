//! Web search and fetch tools with multi-engine ranking and Obscura fallback.

mod common;
pub mod engines;
mod web_fetch;
pub mod ranking;
mod web_search;

#[cfg(feature = "obscura")]
mod obscura;

pub use web_fetch::create_web_fetch_tool;
pub use ranking::{Engine, SearchResult};
pub use web_search::create_web_search_tool;

/// Web tools that do not require an [`ExecutionEnv`].
pub fn create_web_tools() -> Vec<crate::types::AgentTool> {
    vec![create_web_search_tool(), create_web_fetch_tool()]
}
