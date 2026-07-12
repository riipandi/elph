use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::async_util::block_on;
use super::io::{self, create_run_id, update_state_with_run, write_raw_json};
use super::types::{ConnectorId, ConnectorIngestOptions, ConnectorIngestResult, ConnectorRunRecord, IngestStatus};

const HN_FIREBASE: &str = "https://hacker-news.firebaseio.com/v0";
const HN_ALGOLIA: &str = "https://hn.algolia.com/api/v1/search_by_date";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HackerNewsConfig {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_feeds")]
    feeds: Vec<String>,
    #[serde(default = "default_max_items")]
    max_items_per_feed: usize,
    #[serde(default = "default_max_query")]
    max_results_per_query: usize,
    #[serde(default)]
    queries: Vec<String>,
    #[serde(default = "default_tags")]
    query_tags: Vec<String>,
}

fn default_true() -> bool {
    true
}
fn default_feeds() -> Vec<String> {
    vec!["top".into(), "new".into()]
}
fn default_max_items() -> usize {
    30
}
fn default_max_query() -> usize {
    20
}
fn default_tags() -> Vec<String> {
    vec!["story".into()]
}

impl Default for HackerNewsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            feeds: default_feeds(),
            max_items_per_feed: default_max_items(),
            max_results_per_query: default_max_query(),
            queries: Vec::new(),
            query_tags: default_tags(),
        }
    }
}

pub fn ingest(options: ConnectorIngestOptions) -> Result<ConnectorIngestResult> {
    block_on(ingest_async(options))
}

async fn ingest_async(options: ConnectorIngestOptions) -> Result<ConnectorIngestResult> {
    let run_id = create_run_id();
    let config: HackerNewsConfig = io::read_connector_config(ConnectorId::HackerNews)?;
    let mut state = io::read_connector_state(ConnectorId::HackerNews)?;
    let mut warnings = Vec::new();
    let mut raw_files = Vec::new();

    if !config.enabled {
        return Ok(skipped(
            run_id,
            "Hacker News connector is not enabled. Set enabled=true in config.json.",
            warnings,
        ));
    }

    let window_hours = options.window_hours.unwrap_or(24);
    let earliest = chrono::Utc::now().timestamp() - (window_hours as i64 * 3600);
    let feed_limit = options.limit.unwrap_or(config.max_items_per_feed).min(100);
    let query_limit = options.limit.unwrap_or(config.max_results_per_query).min(100);
    let client = http_client();

    let mut feed_results = Vec::new();
    for feed in &config.feeds {
        match fetch_feed(&client, feed, feed_limit, earliest).await {
            Ok(value) => feed_results.push(value),
            Err(err) => warnings.push(format!("{feed}: {err}")),
        }
    }

    let mut query_results = Vec::new();
    for query in &config.queries {
        match search_hn(&client, query, &config.query_tags, query_limit, earliest).await {
            Ok(value) => query_results.push(value),
            Err(err) => warnings.push(format!("{query}: {err}")),
        }
    }

    let path = write_raw_json(
        ConnectorId::HackerNews,
        &run_id,
        "hackernews-results.json",
        &serde_json::json!({
            "fetchedAt": chrono::Utc::now().to_rfc3339(),
            "instanceId": options.instance_id,
            "windowHours": window_hours,
            "feeds": feed_results,
            "queryResults": query_results,
        }),
    )?;
    raw_files.push(path);

    state = update_state_with_run(
        state,
        ConnectorRunRecord {
            at: chrono::Utc::now().to_rfc3339(),
            run_id: run_id.clone(),
            status: IngestStatus::Success.as_str().to_string(),
            raw_files: raw_files.clone(),
            warnings: warnings.clone(),
        },
    );
    io::write_connector_state(ConnectorId::HackerNews, &state)?;

    Ok(ConnectorIngestResult {
        connector_id: ConnectorId::HackerNews,
        message: format!(
            "Fetched {} Hacker News feed(s) and {} search query(ies).",
            feed_results.len(),
            query_results.len()
        ),
        raw_files,
        run_id,
        status: IngestStatus::Success,
        warnings,
    })
}

async fn fetch_feed(client: &reqwest::Client, feed: &str, limit: usize, earliest: i64) -> Result<serde_json::Value> {
    let url = format!("{HN_FIREBASE}/{feed}stories.json");
    let ids: Vec<u64> = client.get(url).send().await?.json().await?;
    let mut items = Vec::new();
    for id in ids.into_iter().take(limit) {
        let item: serde_json::Value = client
            .get(format!("{HN_FIREBASE}/item/{id}.json"))
            .send()
            .await?
            .json()
            .await?;
        if item.get("time").and_then(|v| v.as_i64()).is_some_and(|t| t >= earliest) {
            items.push(item);
        }
    }
    Ok(serde_json::json!({ "feed": feed, "items": items }))
}

async fn search_hn(
    client: &reqwest::Client,
    query: &str,
    tags: &[String],
    limit: usize,
    earliest: i64,
) -> Result<serde_json::Value> {
    let tag = tags.join(",");
    let url = format!(
        "{HN_ALGOLIA}?query={}&tags={}&hitsPerPage={}",
        pct_encode(query),
        pct_encode(&tag),
        limit
    );
    let response: serde_json::Value = client.get(url).send().await?.json().await?;
    let hits = response
        .get("hits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|hit| {
            hit.get("created_at_i")
                .and_then(|v| v.as_i64())
                .is_some_and(|t| t >= earliest)
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({ "query": query, "hits": hits }))
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("reqwest client")
}

fn pct_encode(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

fn skipped(run_id: String, message: &str, warnings: Vec<String>) -> ConnectorIngestResult {
    ConnectorIngestResult {
        connector_id: ConnectorId::HackerNews,
        message: message.to_string(),
        raw_files: Vec::new(),
        run_id,
        status: IngestStatus::Skipped,
        warnings,
    }
}
