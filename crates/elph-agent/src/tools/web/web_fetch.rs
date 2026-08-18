//! `web_fetch` agent tool.

use serde_json::Value;
use serde_json::json;

use elph_ai::Tool;

use crate::tools::common::check_aborted;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

use super::common::{FETCH_MAX_BYTES, parse_public_url};
use super::html::FetchPageResult;

pub fn create_web_fetch_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "web_fetch".into(),
constrained_sampling: None,
            description: "Fetches a URL and returns the content as Markdown. HTML is converted with htmd; JavaScript-heavy pages are returned as fetched (no in-process browser). Useful for providing docs as context.".into(),
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
    let provider_id = parsed
        .host_str()
        .unwrap_or("web-fetch")
        .to_lowercase()
        .replace('.', "-");

    // Check resilience before fetching
    elph_ai::resilience::check_provider_resilience(&provider_id)?;

    log::debug!("web fetch start host={provider_id}");
    let result = match super::html::fetch_page(parsed.as_str()).await {
        Ok(page) => page,
        Err(e) => {
            log::warn!("web fetch failed host={provider_id}: {e:#}");
            elph_ai::resilience::record_provider_failure(&provider_id);
            return Err(e);
        }
    };

    elph_ai::resilience::record_provider_success(&provider_id);

    let page: FetchPageResult = result;
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
