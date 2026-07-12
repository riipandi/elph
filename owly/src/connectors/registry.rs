use super::git_repo;
use super::types::{ConnectorId, ConnectorRuntime};
use super::web_search;
use super::{hackernews, x_source};

pub fn registry() -> Vec<ConnectorRuntime> {
    vec![
        ConnectorRuntime {
            id: ConnectorId::GitRepo,
            display_name: "Local Git repositories",
            description: "Reads local cloned Git repositories and writes compact manifests.",
            required_env: &[],
            ingest: git_repo::ingest,
        },
        ConnectorRuntime {
            id: ConnectorId::HackerNews,
            display_name: "Hacker News",
            description: "Fetches Hacker News feeds and Algolia search results.",
            required_env: &[],
            ingest: hackernews::ingest,
        },
        ConnectorRuntime {
            id: ConnectorId::WebSearch,
            display_name: "Web Search",
            description: "Fetches web search results via Tavily.",
            required_env: &[web_search::TAVILY_API_KEY_ENV],
            ingest: web_search::ingest,
        },
        ConnectorRuntime {
            id: ConnectorId::X,
            display_name: "X / Twitter",
            description: "Fetches X timelines and mentions via API v2.",
            required_env: &[x_source::X_ACCESS_TOKEN_ENV],
            ingest: x_source::ingest,
        },
    ]
}

pub fn get(id: ConnectorId) -> Option<ConnectorRuntime> {
    registry().into_iter().find(|c| c.id == id)
}
