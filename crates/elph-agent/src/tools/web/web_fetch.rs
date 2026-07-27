//! `web_fetch` agent tool.

use serde_json::Value;
use serde_json::json;

use elph_ai::Tool;

use crate::tools::common::check_aborted;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

use super::common::{FETCH_MAX_BYTES, USER_AGENT};
use super::common::{html_to_text, http_client, is_html_content_type, parse_public_url};

#[cfg(feature = "obscura")]
use super::obscura::FetchPageResult;

pub fn create_web_fetch_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "web_fetch".into(),
            description: "Fetches a URL and optionally returns the content as Markdown. HTML is converted to plain text. Falls back to the Obscura headless browser for JavaScript-heavy pages. Useful for providing docs as context.".into(),
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
        "web_fetch",
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
    let provider_id = parsed
        .host_str()
        .unwrap_or("web-fetch")
        .to_lowercase()
        .replace('.', "-");

    // Check resilience before fetching
    elph_ai::resilience::check_provider_resilience(&provider_id)?;

    let result = client
        .get(parsed.clone())
        .header("Accept", "text/html,application/json,text/plain,*/*")
        .header("User-Agent", USER_AGENT)
        .send()
        .await;

    let resp = match result {
        Ok(resp) => resp,
        Err(e) => {
            elph_ai::resilience::record_provider_failure(&provider_id);
            return Err(e.into());
        }
    };

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
        let s = status.as_u16();
        if s == 429 || s >= 500 {
            elph_ai::resilience::record_provider_failure(&provider_id);
        }
        return Err(anyhow::anyhow!("status {status}: {}", trim_body(&body)));
    }

    // Record success for successful fetches
    elph_ai::resilience::record_provider_success(&provider_id);

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
    if trimmed.chars().count() > 240 {
        format!("{}...", super::common::truncate_at_chars(trimmed, 240))
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
