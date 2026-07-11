//! `webfetch` agent tool.

use serde_json::{Value, json};

use elph_ai::Tool;

use crate::tools::common::check_aborted;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

use super::common::{FETCH_MAX_BYTES, USER_AGENT, html_to_text, http_client, is_html_content_type, parse_public_url};

#[cfg(feature = "obscura")]
use super::obscura::FetchPageResult;

pub fn create_webfetch_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "webfetch".into(),
            description: "Fetch content from a public HTTP(S) URL. HTML is converted to plain text. Falls back to the Obscura headless browser for JavaScript-heavy pages.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "HTTP or HTTPS URL to fetch"
                    }
                },
                "required": ["url"]
            }),
        },
        "webfetch",
        |_, args| Box::pin(async move { execute_webfetch(args, None).await }),
    )
}

#[derive(Debug)]
struct FetchResult {
    url: String,
    content_type: String,
    body: String,
}

async fn execute_webfetch(
    args: Value,
    signal: Option<tokio_util::sync::CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;

    let raw_url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: url"))?;

    let result = fetch_url(raw_url).await?;
    Ok(AgentToolResult::text(format_fetch(&result)))
}

async fn fetch_url(raw_url: &str) -> anyhow::Result<FetchResult> {
    let parsed = parse_public_url(raw_url).await?;
    match fetch_http(&parsed).await {
        Ok(result) => Ok(result),
        Err(http_error) => {
            #[cfg(feature = "obscura")]
            {
                match fetch_obscura(parsed.as_str()).await {
                    Ok(result) => Ok(result),
                    Err(obscura_error) => {
                        Err(anyhow::anyhow!("fetch failed (http: {http_error}; obscura: {obscura_error})"))
                    }
                }
            }
            #[cfg(not(feature = "obscura"))]
            {
                Err(http_error)
            }
        }
    }
}

async fn fetch_http(parsed: &url::Url) -> anyhow::Result<FetchResult> {
    let client = http_client();
    let resp = client
        .get(parsed.clone())
        .header("Accept", "text/html,application/json,text/plain,*/*")
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;

    let status = resp.status();
    let final_url = resp.url().to_string();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let bytes = resp.bytes().await?;
    let truncated = bytes.len() > FETCH_MAX_BYTES;
    let data = if truncated { &bytes[..FETCH_MAX_BYTES] } else { &bytes };

    let mut body = String::from_utf8_lossy(data).into_owned();
    if is_html_content_type(&content_type) {
        let text = html_to_text(&body);
        if !text.is_empty() {
            body = text;
        }
    }

    if !status.is_success() {
        return Err(anyhow::anyhow!("status {status}: {}", trim_body(&body)));
    }

    if truncated {
        body.push_str("\n\n(output truncated)");
    }

    Ok(FetchResult {
        url: final_url,
        content_type,
        body: body.trim_end().to_string(),
    })
}

#[cfg(feature = "obscura")]
async fn fetch_obscura(url: &str) -> anyhow::Result<FetchResult> {
    let page: FetchPageResult = super::obscura::fetch_page(url).await?;
    let mut body = page.body;
    if body.len() > FETCH_MAX_BYTES {
        body.truncate(FETCH_MAX_BYTES);
        body.push_str("\n\n(output truncated)");
    }
    Ok(FetchResult {
        url: page.url,
        content_type: page.content_type,
        body: body.trim_end().to_string(),
    })
}

fn format_fetch(result: &FetchResult) -> String {
    let mut output = format!("url: {}\n", result.url);
    if !result.content_type.trim().is_empty() {
        output.push_str(&format!("content_type: {}\n", result.content_type.trim()));
    }
    output.push('\n');
    output.push_str(&result.body);
    output.trim_end().to_string()
}

fn trim_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.len() > 240 {
        format!("{}...", &trimmed[..240])
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_includes_url_and_body() {
        let rendered = format_fetch(&FetchResult {
            url: "https://example.com".into(),
            content_type: "text/plain".into(),
            body: "hello".into(),
        });
        assert!(rendered.contains("url: https://example.com"));
        assert!(rendered.contains("hello"));
    }
}
