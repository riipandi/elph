//! HTTP fetch and HTML extraction for the web tools.
//!
//! Replaces the previous in-process headless-browser backend. Pages are fetched
//! with the shared reqwest client, decoded with `encoding_rs`, and converted to
//! Markdown with `htmd`. DuckDuckGo fallback search is extracted with the
//! lightweight `astral-tl` CSS selector engine — no regex scraping, no browser.

use anyhow::{Context, Result};
use urlencoding::encode;

use super::common::{USER_AGENT, http_client};
use super::ranking::SearchResult;

/// Tags dropped before Markdown conversion. Keeps article/code content, removes
/// chrome, scripts, styles, and non-textual media.
const SKIP_TAGS: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "canvas", "nav", "header", "footer", "aside", "form", "iframe",
    "figure",
];

#[derive(Debug)]
pub struct FetchPageResult {
    pub url: String,
    pub content_type: String,
    pub body: String,
}

/// Fetch a public URL and convert the response body to Markdown.
///
/// Uses the shared reqwest client (timeouts/SSRF protection live in the
/// `common` helpers). Non-UTF-8 bodies are decoded via `encoding_rs`; HTML is
/// converted with `htmd`, skipping layout/chrome tags.
pub async fn fetch_page(raw_url: &str) -> Result<FetchPageResult> {
    let client = http_client();
    let resp = client
        .get(raw_url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .with_context(|| format!("request failed for {raw_url}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow::anyhow!("HTTP {} fetching {raw_url}", status.as_u16()));
    }

    let final_url = resp.url().as_str().to_string();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let bytes = resp.bytes().await.context("read response body")?;
    let html = decode_body(&bytes, &content_type);

    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(SKIP_TAGS.to_vec())
        .scripting_enabled(false)
        .build();
    let body = converter.convert(&html).unwrap_or_else(|_| html.clone());

    Ok(FetchPageResult {
        url: final_url,
        content_type,
        body,
    })
}

/// Best-effort charset decode using the `Content-Type` header when present.
fn decode_body(bytes: &[u8], content_type: &str) -> String {
    let encoding = content_type
        .split(';')
        .find_map(|part| {
            let part = part.trim();
            part.starts_with("charset=")
                .then(|| encoding_rs::Encoding::for_label(&part.as_bytes()["charset=".len()..]))
        })
        .flatten()
        .unwrap_or(encoding_rs::UTF_8);

    if encoding == encoding_rs::UTF_8 {
        return String::from_utf8_lossy(bytes).into_owned();
    }

    let (cow, _had_errors) = encoding.decode_without_bom_handling(bytes);
    cow.into_owned()
}

/// Fallback DuckDuckGo search via the HTML endpoint (no API key, no browser).
///
/// Parsed with `astral-tl`; returns an error when no results are found so the
/// caller can surface a clear failure.
pub async fn search_duckduckgo(query: &str) -> Result<Vec<SearchResult>> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", encode(query));
    let client = http_client();
    let html = client
        .get(&url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .with_context(|| "duckduckgo request failed".to_string())?
        .error_for_status()
        .with_context(|| "duckduckgo returned an error status")?
        .text()
        .await
        .context("read duckduckgo response")?;

    let results = parse_ddg_results(&html);
    if results.is_empty() {
        return Err(anyhow::anyhow!("duckduckgo: no results"));
    }
    Ok(results)
}

/// Extract DuckDuckGo results with a CSS selector pass.
fn parse_ddg_results(html: &str) -> Vec<SearchResult> {
    let dom = match tl::parse(html, tl::ParserOptions::default()) {
        Ok(dom) => dom,
        Err(_) => return Vec::new(),
    };
    let parser = dom.parser();

    // Collect every result link, then pair it with the following snippet in
    // document order. DuckDuckGo emits them as siblings inside `.result`.
    let mut links: Vec<(String, String)> = Vec::new();
    let mut snippets: Vec<String> = Vec::new();

    for node in dom.nodes() {
        let Some(tag) = node.as_tag() else {
            continue;
        };
        let classes = tag
            .attributes()
            .get("class")
            .flatten()
            .map(|c| c.as_utf8_str().to_string())
            .unwrap_or_default();

        if classes.split_whitespace().any(|c| c == "result__a") {
            let url = tag
                .attributes()
                .get("href")
                .flatten()
                .map(|a| a.as_utf8_str().to_string())
                .unwrap_or_default();
            let title = node.inner_text(parser).trim().to_string();
            if !url.is_empty() && !title.is_empty() {
                links.push((title, url));
            }
        } else if classes.split_whitespace().any(|c| c == "result__snippet") {
            snippets.push(node.inner_text(parser).trim().to_string());
        }
    }

    let n = links.len().min(snippets.len());
    let mut results = Vec::with_capacity(n);
    for i in 0..n {
        let (title, url) = links[i].clone();
        results.push(SearchResult {
            title,
            url,
            snippet: snippets[i].clone(),
            content: None,
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ddg_results_extracts_links() {
        let html = r#"<body>
            <div class="result">
                <a class="result__a" href="https://example.com">Example <b>Site</b></a>
                <a class="result__snippet">A short <i>snippet</i> here</a>
            </div>
        </body>"#;
        let results = parse_ddg_results(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example Site");
        assert_eq!(results[0].url, "https://example.com");
        assert_eq!(results[0].snippet, "A short snippet here");
    }

    #[test]
    fn parse_ddg_results_handles_empty() {
        assert!(parse_ddg_results("<body></body>").is_empty());
    }
}
