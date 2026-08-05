//! Crawlberg-backed web fetch and search.
//!
//! Single in-process path via `crawlberg` (native browser backend derived from
//! Obscura). There is no dedicated worker thread and no `dup2` stderr
//! redirection — `crawlberg` runs the browser inside the async runtime.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use crawlberg::{BrowserBackend, BrowserConfig, BrowserMode, CrawlConfig, CrawlEngineHandle, create_engine, scrape};
use urlencoding::encode;

use super::common::USER_AGENT;
use super::engines::parse_ddg_html;
use super::ranking::SearchResult;

#[derive(Debug)]
pub struct FetchPageResult {
    pub url: String,
    pub content_type: String,
    pub body: String,
}

/// Engine that escalates to the browser only when JS rendering is needed.
static AUTO_ENGINE: OnceLock<CrawlEngineHandle> = OnceLock::new();
/// Engine that always routes through the browser (used for DuckDuckGo, which
/// blocks plain HTTP fetches).
static ALWAYS_ENGINE: OnceLock<CrawlEngineHandle> = OnceLock::new();

fn native_config(mode: BrowserMode) -> CrawlConfig {
    CrawlConfig::builder()
        .user_agent(USER_AGENT)
        .browser(BrowserConfig {
            mode,
            backend: BrowserBackend::Native,
            ..Default::default()
        })
        .build()
}

fn engine(slot: &'static OnceLock<CrawlEngineHandle>, mode: BrowserMode) -> Result<&'static CrawlEngineHandle> {
    if let Some(handle) = slot.get() {
        return Ok(handle);
    }
    let handle = create_engine(Some(native_config(mode))).map_err(|e| anyhow::anyhow!("crawlberg engine init: {e}"))?;
    Ok(slot.get_or_init(|| handle))
}

fn auto_engine() -> Result<&'static CrawlEngineHandle> {
    engine(&AUTO_ENGINE, BrowserMode::Auto)
}

fn always_engine() -> Result<&'static CrawlEngineHandle> {
    engine(&ALWAYS_ENGINE, BrowserMode::Always)
}

pub async fn fetch_page(url: &str) -> Result<FetchPageResult> {
    let engine = auto_engine().context("crawlberg engine")?;
    let result = scrape(engine, url)
        .await
        .map_err(|e| anyhow::anyhow!("crawlberg scrape: {e}"))?;

    if !(200..300).contains(&result.status_code) {
        return Err(anyhow::anyhow!("crawlberg: status {} fetching {url}", result.status_code));
    }

    let body = match result.markdown {
        Some(markdown) => markdown.content,
        None => result.html,
    };

    Ok(FetchPageResult {
        url: result.final_url,
        content_type: result.content_type,
        body,
    })
}

pub async fn search_duckduckgo(query: &str) -> Result<Vec<SearchResult>> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", encode(query));
    let engine = always_engine().context("crawlberg engine")?;
    let result = scrape(engine, &url)
        .await
        .map_err(|e| anyhow::anyhow!("crawlberg: {e}"))?;
    let results = parse_ddg_html(&result.html);
    if results.is_empty() {
        return Err(anyhow::anyhow!("crawlberg: no duckduckgo results"));
    }
    Ok(results)
}
