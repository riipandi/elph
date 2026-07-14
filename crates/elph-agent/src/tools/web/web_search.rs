//! `web_search` agent tool.

use serde_json::{Value, json};

use elph_ai::Tool;

use crate::tools::common::check_aborted;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

use super::common::http_client;
use super::engines::search_engine;
use super::ranking::{Engine, format_results, ordered_try_list};

#[cfg(feature = "obscura")]
use super::obscura;

pub fn create_web_search_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "web_search".into(),
            description: "Searches the web for information, providing results with snippets and links from relevant web pages. Supports multiple engines with automatic ranking and fallback. Useful for accessing real-time information.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query string"
                    },
                    "engine": {
                        "type": "string",
                        "description": "Preferred engine: auto, duckduckgo, brave, exa, firecrawl, jina, perplexity, tavily, serpapi"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of results (default: 5, max: 20)"
                    }
                },
                "required": ["query"]
            }),
        },
        "web_search",
        |_, args| Box::pin(async move { execute_websearch(args, None).await }),
    )
}

async fn execute_websearch(
    args: Value,
    signal: Option<tokio_util::sync::CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;

    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: query"))?;
    if query.trim().is_empty() {
        return Err(anyhow::anyhow!("Empty search query"));
    }

    let preferred = args
        .get("engine")
        .and_then(|v| v.as_str())
        .and_then(Engine::from_str_opt);

    if let Some(pref) = preferred
        && !pref.is_available()
        && let Some(env_var) = pref.key_env()
    {
        return Err(anyhow::anyhow!("{} requires {}", pref.name(), env_var));
    }

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5).clamp(1, 20) as usize;

    let (engine, results) = run_search(query, preferred, limit).await?;
    Ok(AgentToolResult::text(format_results(engine, query, &results)))
}

async fn run_search(
    query: &str,
    preferred: Option<Engine>,
    limit: usize,
) -> anyhow::Result<(Engine, Vec<super::ranking::SearchResult>)> {
    let client = http_client();
    let engines = ordered_try_list(preferred);
    let mut errors = Vec::new();

    for engine in engines {
        let api_key = engine.api_key();
        match search_engine(client, engine, query, &api_key).await {
            Ok(results) if !results.is_empty() => {
                let limited = if results.len() > limit {
                    results[..limit].to_vec()
                } else {
                    results
                };
                return Ok((engine, limited));
            }
            Ok(_) => errors.push(format!("{}: no results", engine.name())),
            Err(error) => errors.push(format!("{}: {error}", engine.name())),
        }
    }

    #[cfg(feature = "obscura")]
    {
        match obscura::search_duckduckgo(query).await {
            Ok(results) if !results.is_empty() => {
                let limited = if results.len() > limit {
                    results[..limit].to_vec()
                } else {
                    results
                };
                return Ok((Engine::DuckDuckGo, limited));
            }
            Ok(_) => errors.push("Obscura: no results".into()),
            Err(error) => errors.push(format!("Obscura: {error}")),
        }
    }

    Err(anyhow::anyhow!("web search failed: {}", errors.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_empty_query() {
        let err = execute_websearch(json!({ "query": "  " }), None).await.unwrap_err();
        assert!(err.to_string().contains("Empty search query"));
    }
}
