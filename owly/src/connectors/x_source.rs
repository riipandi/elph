use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::async_util::block_on;
use super::io::{self, create_run_id, update_state_with_run, write_raw_json};
use super::types::{ConnectorId, ConnectorIngestOptions, ConnectorIngestResult, ConnectorRunRecord, IngestStatus};

pub const X_ACCESS_TOKEN_ENV: &str = "OWLY_X_ACCESS_TOKEN";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct XConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_streams")]
    streams: Vec<String>,
    #[serde(default = "default_pages")]
    max_pages_per_stream: u32,
}

fn default_streams() -> Vec<String> {
    vec!["home_timeline".into(), "user_posts".into(), "mentions".into()]
}
fn default_pages() -> u32 {
    2
}

impl Default for XConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            streams: default_streams(),
            max_pages_per_stream: default_pages(),
        }
    }
}

pub fn ingest(options: ConnectorIngestOptions) -> Result<ConnectorIngestResult> {
    block_on(ingest_async(options))
}

async fn ingest_async(options: ConnectorIngestOptions) -> Result<ConnectorIngestResult> {
    let run_id = create_run_id();
    let config: XConfig = io::read_connector_config(ConnectorId::X)?;
    let mut state = io::read_connector_state(ConnectorId::X)?;
    let mut warnings = Vec::new();
    let mut raw_files = Vec::new();

    if !config.enabled {
        return Ok(skipped(
            run_id,
            "X connector is not enabled. Run `owly auth configure x` and set enabled=true.",
            warnings,
        ));
    }

    let token = std::env::var(X_ACCESS_TOKEN_ENV)
        .or_else(|_| std::env::var("OPENWIKI_X_ACCESS_TOKEN"))
        .context(format!("{X_ACCESS_TOKEN_ENV} is required for X ingestion"))?;

    let client = reqwest::Client::new();
    let mut stream_results = Vec::new();

    for stream in &config.streams {
        let url = match stream.as_str() {
            "home_timeline" => "https://api.x.com/2/users/me/timelines/reverse_chronological",
            "mentions" => "https://api.x.com/2/users/me/mentions",
            "user_posts" => match fetch_user_id(&client, &token).await {
                Ok(user_id) => {
                    let url = format!("https://api.x.com/2/users/{user_id}/tweets");
                    match fetch_x_page(&client, &token, &url).await {
                        Ok(page) => stream_results.push(serde_json::json!({ "stream": stream, "page": page })),
                        Err(err) => warnings.push(format!("{stream}: {err}")),
                    }
                    continue;
                }
                Err(err) => {
                    warnings.push(format!("{stream}: {err}"));
                    continue;
                }
            },
            other => {
                warnings.push(format!("Unsupported X stream: {other}"));
                continue;
            }
        };
        match fetch_x_page(&client, &token, url).await {
            Ok(page) => stream_results.push(serde_json::json!({ "stream": stream, "page": page })),
            Err(err) => warnings.push(format!("{stream}: {err}")),
        }
    }

    let path = write_raw_json(
        ConnectorId::X,
        &run_id,
        "x-results.json",
        &serde_json::json!({
            "fetchedAt": chrono::Utc::now().to_rfc3339(),
            "instanceId": options.instance_id,
            "streams": stream_results,
        }),
    )?;
    raw_files.push(path);

    let status = if stream_results.is_empty() {
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
    io::write_connector_state(ConnectorId::X, &state)?;

    Ok(ConnectorIngestResult {
        connector_id: ConnectorId::X,
        message: format!("Fetched {} X stream(s).", stream_results.len()),
        raw_files,
        run_id,
        status,
        warnings,
    })
}

async fn fetch_user_id(client: &reqwest::Client, token: &str) -> Result<String> {
    let resp: serde_json::Value = client
        .get("https://api.x.com/2/users/me")
        .bearer_auth(token)
        .send()
        .await?
        .json()
        .await?;
    resp.get("data")
        .and_then(|d| d.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .context("missing user id in X API response")
}

async fn fetch_x_page(client: &reqwest::Client, token: &str, url: &str) -> Result<serde_json::Value> {
    let full_url = if url.contains('?') {
        format!("{url}&max_results=20&tweet.fields=created_at,author_id")
    } else {
        format!("{url}?max_results=20&tweet.fields=created_at,author_id")
    };
    client
        .get(full_url)
        .bearer_auth(token)
        .send()
        .await?
        .json()
        .await
        .context("X API response")
}

fn skipped(run_id: String, message: &str, warnings: Vec<String>) -> ConnectorIngestResult {
    ConnectorIngestResult {
        connector_id: ConnectorId::X,
        message: message.to_string(),
        raw_files: Vec::new(),
        run_id,
        status: IngestStatus::Skipped,
        warnings,
    }
}
