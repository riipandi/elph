//! Obscura headless browser worker — runs on a dedicated thread via crossbeam-channel.

use anyhow::{Context, Result};
use crossbeam_channel::unbounded;
use crossbeam_channel::{Receiver, Sender};
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

/// Suppress stderr for the Obscura browser thread by redirecting fd 2 to /dev/null.
///
/// The V8 engine inside Obscura writes JavaScript console output and HTTP fetch
/// errors directly to file descriptor 2, bypassing the Rust `log` crate.  In TUI
/// mode these messages corrupt the terminal; after exit they leak to the raw shell.
/// This function discards them so the logging system (file + logforth) stays the
/// sole output channel.
#[cfg(unix)]
fn suppress_thread_stderr() {
    use std::os::unix::io::AsRawFd;
    // FFI declaration avoids pulling in `libc` as a direct dependency.
    extern "C" {
        fn dup2(oldfd: std::os::raw::c_int, newfd: std::os::raw::c_int) -> std::os::raw::c_int;
    }
    if let Ok(null) = std::fs::File::open("/dev/null") {
        // SAFETY: `null` was just opened, so its fd is valid. fd 2 (stderr)
        // is guaranteed to be open in a running process. dup2 is
        // signal-safe. Called once at worker start — no concurrent
        // dup2/close racing on fd 2 at this point.
        unsafe {
            dup2(null.as_raw_fd(), 2);
        }
    }
}

#[cfg(not(unix))]
fn suppress_thread_stderr() {}

fn run_browser_worker(rx: Receiver<BrowserJob>) {
    suppress_thread_stderr();
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
