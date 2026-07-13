//! Obscura headless browser worker — runs on a dedicated thread via crossbeam-channel.

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, unbounded};
use obscura::Browser;
use std::sync::OnceLock;
use std::thread;
use tokio::sync::oneshot;

use super::common::{html_to_text, strip_html};
use super::engines::parse_ddg_html;
use super::ranking::SearchResult;

#[derive(Debug)]
pub struct FetchPageResult {
    pub url: String,
    pub content_type: String,
    pub body: String,
}

enum BrowserJob {
    FetchPage {
        url: String,
        reply: oneshot::Sender<Result<FetchPageResult>>,
    },
    SearchDuckDuckGo {
        query: String,
        reply: oneshot::Sender<Result<Vec<SearchResult>>>,
    },
}

struct BrowserWorker {
    tx: Sender<BrowserJob>,
}

static WORKER: OnceLock<BrowserWorker> = OnceLock::new();

fn worker() -> &'static BrowserWorker {
    WORKER.get_or_init(|| {
        let (tx, rx) = unbounded();
        thread::Builder::new()
            .name("obscura-browser".into())
            .spawn(move || run_browser_worker(rx))
            .expect("failed to spawn obscura browser worker");
        BrowserWorker { tx }
    })
}

pub async fn fetch_page(url: &str) -> Result<FetchPageResult> {
    let (reply_tx, reply_rx) = oneshot::channel();
    worker()
        .tx
        .send(BrowserJob::FetchPage {
            url: url.to_string(),
            reply: reply_tx,
        })
        .map_err(|_| anyhow::anyhow!("obscura browser worker stopped"))?;
    reply_rx
        .await
        .map_err(|_| anyhow::anyhow!("obscura browser worker dropped reply"))?
}

pub async fn search_duckduckgo(query: &str) -> Result<Vec<SearchResult>> {
    let (reply_tx, reply_rx) = oneshot::channel();
    worker()
        .tx
        .send(BrowserJob::SearchDuckDuckGo {
            query: query.to_string(),
            reply: reply_tx,
        })
        .map_err(|_| anyhow::anyhow!("obscura browser worker stopped"))?;
    reply_rx
        .await
        .map_err(|_| anyhow::anyhow!("obscura browser worker dropped reply"))?
}

fn run_browser_worker(rx: Receiver<BrowserJob>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("obscura browser runtime");

    rt.block_on(async {
        let browser = match Browser::builder().stealth(true).build() {
            Ok(browser) => browser,
            Err(error) => {
                log::error!("failed to start obscura browser: {error}");
                return;
            }
        };

        while let Ok(job) = rx.recv() {
            match job {
                BrowserJob::FetchPage { url, reply } => {
                    let _ = reply.send(fetch_page_inner(&browser, &url).await);
                }
                BrowserJob::SearchDuckDuckGo { query, reply } => {
                    let _ = reply.send(search_ddg_inner(&browser, &query).await);
                }
            }
        }
    });
}

async fn fetch_page_inner(browser: &Browser, url: &str) -> Result<FetchPageResult> {
    let mut page = browser.new_page().await.context("obscura: new page")?;
    page.goto(url).await.context("obscura: navigate")?;
    page.settle(2_000).await;
    let final_url = page.url();
    let html = page.content();
    let body = html_to_text(&html);
    let body = if body.is_empty() { strip_html(&html) } else { body };
    Ok(FetchPageResult {
        url: final_url,
        content_type: "text/html".into(),
        body,
    })
}

async fn search_ddg_inner(browser: &Browser, query: &str) -> Result<Vec<SearchResult>> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding::encode(query));
    let mut page = browser.new_page().await.context("obscura: new page")?;
    page.goto(&url).await.context("obscura: navigate")?;
    page.settle(2_000).await;
    let html = page.content();
    let results = parse_ddg_html(&html);
    if results.is_empty() {
        return Err(anyhow::anyhow!("obscura: no duckduckgo results"));
    }
    Ok(results)
}
