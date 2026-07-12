use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::async_util::block_on;
use super::io::{self, create_run_id, update_state_with_run, write_raw_json};
use super::types::{ConnectorId, ConnectorIngestOptions, ConnectorIngestResult, ConnectorRunRecord, IngestStatus};

pub const TAVILY_API_KEY_ENV: &str = "TAVILY_API_KEY";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebSearchConfig {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    queries: Vec<String>,
    #[serde(default = "default_max_results")]
    max_results: u32,
    #[serde(default = "default_depth")]
    search_depth: String,
}

fn default_true() -> bool {
    true
}
fn default_max_results() -> u32 {
    5
}
fn default_depth() -> String {
    "basic".into()
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            queries: Vec::new(),
            max_results: default_max_results(),
            search_depth: default_depth(),
        }
    }
}

pub fn ingest(options: ConnectorIngestOptions) -> Result<ConnectorIngestResult> {
    block_on(ingest_async(options))
}

async fn ingest_async(options: ConnectorIngestOptions) -> Result<ConnectorIngestResult> {
    let run_id = create_run_id();
    let mut config: WebSearchConfig = io::read_connector_config(ConnectorId::WebSearch)?;
    if let Some(override_cfg) = options.connector_config
        && let Ok(extra) = serde_json::from_value::<WebSearchConfig>(override_cfg)
        && !extra.queries.is_empty()
    {
        config.queries = extra.queries;
    }
    let mut state = io::read_connector_state(ConnectorId::WebSearch)?;
    let mut warnings = Vec::new();
    let mut raw_files = Vec::new();

    if !config.enabled {
        return Ok(skipped(
            run_id,
            "Web Search connector is not enabled. Set enabled=true in config.json.",
            warnings,
        ));
    }

    let api_key = std::env::var(TAVILY_API_KEY_ENV).context(format!("{TAVILY_API_KEY_ENV} is required"))?;

    if config.queries.is_empty() {
        return Ok(skipped(
            run_id,
            "No search queries configured. Add queries to ~/.owly/connectors/web-search/config.json.",
            warnings,
        ));
    }

    let client = reqwest::Client::new();
    let mut results = Vec::new();
    for query in &config.queries {
        let body = serde_json::json!({
            "api_key": api_key,
            "query": query,
            "max_results": config.max_results,
            "search_depth": config.search_depth,
        });
        match client.post("https://api.tavily.com/search").json(&body).send().await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(json) => results.push(serde_json::json!({ "query": query, "response": json })),
                Err(err) => warnings.push(format!("{query}: {err}")),
            },
            Err(err) => warnings.push(format!("{query}: {err}")),
        }
    }

    let path = write_raw_json(
        ConnectorId::WebSearch,
        &run_id,
        "web-search-results.json",
        &serde_json::json!({
            "fetchedAt": chrono::Utc::now().to_rfc3339(),
            "instanceId": options.instance_id,
            "results": results,
        }),
    )?;
    raw_files.push(path);

    let status = if results.is_empty() {
        IngestStatus::Error
    } else {
        IngestStatus::Success
    };

    state = update_state_with_run(
        state,
        ConnectorRunRecord {
            at: chrono::Utc::now().to_rfc3339(),
            run_id: run_id.clone(),
            status: status.as_str().to_string(),
            raw_files: raw_files.clone(),
            warnings: warnings.clone(),
        },
    );
    io::write_connector_state(ConnectorId::WebSearch, &state)?;

    Ok(ConnectorIngestResult {
        connector_id: ConnectorId::WebSearch,
        message: format!("Fetched {} web search quer(ies).", results.len()),
        raw_files,
        run_id,
        status,
        warnings,
    })
}

fn skipped(run_id: String, message: &str, warnings: Vec<String>) -> ConnectorIngestResult {
    ConnectorIngestResult {
        connector_id: ConnectorId::WebSearch,
        message: message.to_string(),
        raw_files: Vec::new(),
        run_id,
        status: IngestStatus::Skipped,
        warnings,
    }
}
