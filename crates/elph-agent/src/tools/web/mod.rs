//! Web search and fetch tools with multi-engine ranking.

mod common;
pub mod engines;
mod html;
pub mod ranking;
mod web_extract;
mod web_fetch;
mod web_search;

pub use ranking::{Engine, SearchResult};
pub use web_extract::create_web_extract_tool;
pub use web_fetch::create_web_fetch_tool;
pub use web_search::create_web_search_tool;

/// Web tools that do not require an [`ExecutionEnv`].
pub fn create_web_tools() -> Vec<crate::types::AgentTool> {
    vec![
        create_web_search_tool(),
        create_web_fetch_tool(),
        create_web_extract_tool(),
    ]
}
