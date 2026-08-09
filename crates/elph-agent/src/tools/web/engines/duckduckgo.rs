//! DuckDuckGo HTML search — no API key required.
//!
//! Uses the public HTML endpoints (not Instant Answer). Datacenter IPs are often
//! CAPTCHA-walled; failures surface as bot-challenge errors so auto mode can fall
//! through to keyed engines instead of reporting empty results.

use std::sync::OnceLock;

use regex::Regex;
use reqwest::Client;

use super::super::common::{BROWSER_USER_AGENT, bot_challenge_error, detect_bot_challenge, do_get, strip_html};
use super::super::ranking::SearchResult;

const HTML_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const LITE_ENDPOINT: &str = "https://lite.duckduckgo.com/lite/";

/// Browser-like headers reduce (but do not eliminate) bot challenges on DDG HTML.
fn ddg_headers() -> [(&'static str, &'static str); 4] {
    [
        ("User-Agent", BROWSER_USER_AGENT),
        ("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
        ("Accept-Language", "en-US,en;q=0.9"),
        ("Referer", "https://html.duckduckgo.com/"),
    ]
}

pub async fn search(client: &Client, query: &str) -> anyhow::Result<Vec<SearchResult>> {
    // Prefer POST form (how the DDG HTML UI submits). GET often trips anomaly checks.
    match search_html_post(client, query).await {
        Ok(results) if !results.is_empty() => return Ok(results),
        Ok(_) => {}
        Err(err) if is_bot_block(&err) => return Err(err),
        Err(_) => {}
    }

    // GET html endpoint.
    let get_url = format!("{HTML_ENDPOINT}?q={}", urlencoding::encode(query));
    match fetch_and_parse(client, &get_url).await {
        Ok(results) if !results.is_empty() => return Ok(results),
        Ok(_) => {}
        Err(err) if is_bot_block(&err) => return Err(err),
        Err(_) => {}
    }

    // Lite endpoint as last HTML attempt (different markup — use lite parser).
    let lite_url = format!("{LITE_ENDPOINT}?q={}", urlencoding::encode(query));
    match fetch_and_parse_lite(client, &lite_url).await {
        Ok(results) if !results.is_empty() => Ok(results),
        Ok(_) => Err(anyhow::anyhow!("duckduckgo: no results")),
        Err(err) => Err(err),
    }
}

fn is_bot_block(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("captcha") || msg.contains("bot") || msg.contains("challenge")
}

async fn search_html_post(client: &Client, query: &str) -> anyhow::Result<Vec<SearchResult>> {
    let body = format!("q={}&b=", urlencoding::encode(query));
    let mut req = client
        .post(HTML_ENDPOINT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body);
    for (k, v) in ddg_headers() {
        req = req.header(k, v);
    }
    let resp = crate::trace::with_trace_headers(req).send().await?;
    let status = resp.status();
    let html = resp.text().await?;
    if !status.is_success() {
        if let Some(reason) = detect_bot_challenge(&html) {
            return Err(bot_challenge_error("DuckDuckGo", reason));
        }
        return Err(anyhow::anyhow!("HTTP {status}: duckduckgo post failed"));
    }
    parse_or_challenge(&html)
}

async fn fetch_and_parse(client: &Client, url: &str) -> anyhow::Result<Vec<SearchResult>> {
    let html = do_get(client, url, &ddg_headers()).await?;
    parse_or_challenge(&html)
}

async fn fetch_and_parse_lite(client: &Client, url: &str) -> anyhow::Result<Vec<SearchResult>> {
    let html = do_get(client, url, &ddg_headers()).await?;
    if let Some(reason) = detect_bot_challenge(&html) {
        return Err(bot_challenge_error("DuckDuckGo", reason));
    }
    let results = parse_ddg_lite_html(&html);
    if results.is_empty() && looks_like_challenge_empty(&html) {
        return Err(bot_challenge_error("DuckDuckGo", "empty page (likely bot wall)"));
    }
    Ok(results)
}

fn parse_or_challenge(html: &str) -> anyhow::Result<Vec<SearchResult>> {
    if let Some(reason) = detect_bot_challenge(html) {
        return Err(bot_challenge_error("DuckDuckGo", reason));
    }
    let results = parse_ddg_html(html);
    if results.is_empty() && looks_like_challenge_empty(html) {
        return Err(bot_challenge_error("DuckDuckGo", "empty page (likely bot wall)"));
    }
    Ok(results)
}

fn looks_like_challenge_empty(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    // No result markers and the page still looks like HTML — often a challenge shell.
    !lower.contains("result__a")
        && !lower.contains("result-link")
        && (lower.contains("<html") || lower.contains("<!doctype"))
        && html.len() > 200
}

pub fn parse_ddg_html(html: &str) -> Vec<SearchResult> {
    static LINK_RE: OnceLock<Regex> = OnceLock::new();
    static SNIPPET_RE: OnceLock<Regex> = OnceLock::new();
    let link_re = LINK_RE.get_or_init(|| {
        Regex::new(r#"<a[^>]*class="result__a"[^>]*href="([^"]*)"[^>]*>([\s\S]*?)</a>"#).expect("ddg link regex")
    });
    let snippet_re = SNIPPET_RE.get_or_init(|| {
        Regex::new(r#"<a[^>]*class="result__snippet"[^>]*>([\s\S]*?)</a>"#).expect("ddg snippet regex")
    });

    let links: Vec<_> = link_re.captures_iter(html).collect();
    let snippets: Vec<_> = snippet_re.captures_iter(html).collect();
    // Snippets are optional — DDG sometimes omits them; don't drop title/url pairs.
    let mut results = Vec::with_capacity(links.len());
    for (i, link) in links.iter().enumerate() {
        let url = link.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let title = strip_html(link.get(2).map(|m| m.as_str()).unwrap_or(""));
        let snippet = snippets
            .get(i)
            .and_then(|c| c.get(1))
            .map(|m| strip_html(m.as_str()))
            .unwrap_or_default();
        if !url.is_empty() && !title.is_empty() {
            results.push(SearchResult {
                title,
                url,
                snippet,
                content: None,
            });
        }
    }
    results
}

/// Parse DuckDuckGo Lite HTML (`lite.duckduckgo.com/lite/`).
fn parse_ddg_lite_html(html: &str) -> Vec<SearchResult> {
    // Lite uses `class="result-link"`; attribute order varies (href before/after class).
    static LINK_RE: OnceLock<Regex> = OnceLock::new();
    let link_re = LINK_RE.get_or_init(|| {
        Regex::new(r#"<a\s+[^>]*class="result-link"[^>]*href="([^"]*)"[^>]*>([\s\S]*?)</a>"#)
            .expect("ddg lite link regex")
    });
    static LINK_RE_ALT: OnceLock<Regex> = OnceLock::new();
    let link_re_alt = LINK_RE_ALT.get_or_init(|| {
        Regex::new(r#"<a\s+[^>]*href="([^"]*)"[^>]*class="result-link"[^>]*>([\s\S]*?)</a>"#)
            .expect("ddg lite alt regex")
    });

    let mut results = Vec::new();
    for caps in link_re.captures_iter(html).chain(link_re_alt.captures_iter(html)) {
        let url = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let title = strip_html(caps.get(2).map(|m| m.as_str()).unwrap_or(""));
        if !url.is_empty() && !title.is_empty() && !results.iter().any(|r: &SearchResult| r.url == url) {
            results.push(SearchResult {
                title,
                url,
                snippet: String::new(),
                content: None,
            });
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::super::super::common::detect_bot_challenge;
    use super::*;

    #[test]
    fn parse_ddg_results() {
        let html = r#"<a class="result__a" href="https://example.com">Example <b>Site</b></a>
<a class="result__snippet">A short <i>snippet</i> here</a>"#;
        let results = parse_ddg_html(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example Site");
        assert_eq!(results[0].url, "https://example.com");
        assert_eq!(results[0].snippet, "A short snippet here");
    }

    #[test]
    fn parse_ddg_results_without_snippet_still_keeps_link() {
        let html = r#"<a class="result__a" href="https://example.com">Only Title</a>"#;
        let results = parse_ddg_html(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Only Title");
        assert!(results[0].snippet.is_empty());
    }

    #[test]
    fn captcha_page_is_detected() {
        let html = r#"<html><body class="anomaly-modal">Please complete the following challenge</body></html>"#;
        assert!(detect_bot_challenge(html).is_some());
        assert!(parse_or_challenge(html).is_err());
        let err = parse_or_challenge(html).unwrap_err().to_string();
        assert!(err.to_ascii_lowercase().contains("captcha") || err.to_ascii_lowercase().contains("challenge"));
    }

    #[test]
    fn normal_results_not_flagged_as_captcha() {
        let html = r#"<div class="results"><a class="result__a" href="https://x.test">X</a>
<a class="result__snippet">snippet</a></div>"#;
        assert!(detect_bot_challenge(html).is_none());
        assert_eq!(parse_or_challenge(html).unwrap().len(), 1);
    }
}
