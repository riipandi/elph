//! Personal-mode connectors (git-repo, web-search, hackernews, x).

mod async_util;
mod git_repo;
mod hackernews;
pub mod io;
mod registry;
mod types;
mod web_search;
mod x_source;

pub use io::{config_path, connectors_root, ensure_connector_home};
pub use registry::{get, registry};
pub use types::{
    CONNECTOR_IDS, ConnectorId, ConnectorIngestOptions, ConnectorIngestResult, IngestStatus, is_connector_id,
    is_safe_source_instance_id,
};
pub use web_search::TAVILY_API_KEY_ENV;
pub use x_source::X_ACCESS_TOKEN_ENV;

pub fn default_connector_config(id: ConnectorId) -> serde_json::Value {
    match id {
        ConnectorId::GitRepo => serde_json::json!({ "repos": [] }),
        ConnectorId::HackerNews => serde_json::json!({
            "enabled": true,
            "feeds": ["top", "new"],
            "maxItemsPerFeed": 30,
            "maxResultsPerQuery": 20,
            "queries": [],
            "queryTags": ["story"]
        }),
        ConnectorId::WebSearch => serde_json::json!({
            "enabled": true,
            "queries": [],
            "maxResults": 5,
            "searchDepth": "basic"
        }),
        ConnectorId::X => serde_json::json!({
            "enabled": false,
            "streams": ["home_timeline", "user_posts", "mentions"],
            "maxPagesPerStream": 2
        }),
    }
}
