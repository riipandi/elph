//! Connector types (OpenWiki `connectors/types.ts` subset).

use serde::{Deserialize, Serialize};

/// Supported Owly connector ids (excludes slack, google/gmail, notion).
pub const CONNECTOR_IDS: &[&str] = &["git-repo", "x", "web-search", "hackernews"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectorId {
    #[serde(rename = "git-repo")]
    GitRepo,
    X,
    #[serde(rename = "web-search")]
    WebSearch,
    HackerNews,
}

impl ConnectorId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GitRepo => "git-repo",
            Self::X => "x",
            Self::WebSearch => "web-search",
            Self::HackerNews => "hackernews",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "git-repo" => Some(Self::GitRepo),
            "x" => Some(Self::X),
            "web-search" => Some(Self::WebSearch),
            "hackernews" => Some(Self::HackerNews),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::GitRepo => "Local Git repositories",
            Self::X => "X / Twitter",
            Self::WebSearch => "Web Search",
            Self::HackerNews => "Hacker News",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectorState {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub runs: Vec<ConnectorRunRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_ids: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorRunRecord {
    pub at: String,
    pub run_id: String,
    pub status: String,
    #[serde(default)]
    pub raw_files: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ConnectorIngestOptions {
    pub instance_id: Option<String>,
    pub window_hours: Option<u32>,
    pub limit: Option<usize>,
    pub connector_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ConnectorIngestResult {
    pub connector_id: ConnectorId,
    pub message: String,
    pub raw_files: Vec<String>,
    pub run_id: String,
    pub status: IngestStatus,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestStatus {
    Success,
    Skipped,
    Error,
}

impl IngestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Skipped => "skipped",
            Self::Error => "error",
        }
    }
}

pub type IngestFn = fn(ConnectorIngestOptions) -> anyhow::Result<ConnectorIngestResult>;

#[derive(Debug, Clone)]
pub struct ConnectorRuntime {
    pub id: ConnectorId,
    pub display_name: &'static str,
    pub description: &'static str,
    pub required_env: &'static [&'static str],
    pub ingest: IngestFn,
}

pub fn is_connector_id(value: &str) -> bool {
    ConnectorId::parse(value).is_some()
}

pub fn is_safe_source_instance_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}
