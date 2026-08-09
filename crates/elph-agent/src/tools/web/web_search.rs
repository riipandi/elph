//! `web_search` agent tool.

use serde_json::Value;
use serde_json::json;

use elph_ai::Tool;

use crate::tools::common::check_aborted;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

use super::common::http_client;
use super::engines::search_engine;
use super::html::search_duckduckgo;
use super::ranking::Engine;
use super::ranking::{format_results, ordered_try_list};

pub fn create_web_search_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "web_search".into(),
            constrained_sampling: None,
            description: "Searches the web for information, providing results with snippets and links from relevant web pages. \
Supports multiple engines with automatic ranking and fallback. \
When `engine` is set to a specific provider (not `auto`), only that engine is used — no silent switch to another provider. \
Useful for accessing real-time information."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query string"
                    },
                    "engine": {
                        "type": "string",
                        "description": "Search engine. Use `auto` (default) to try configured engines by rank with fallback. \
Set an explicit engine (`duckduckgo`/`ddg`, `brave`, `exa`, `firecrawl`, `jina`, `perplexity`, `tavily`, `serpapi`) \
to use only that provider — failure does not switch engines. \
HTML DuckDuckGo may be CAPTCHA-walled from datacenter IPs; prefer API engines when available."
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

/// Parse the optional `engine` argument.
///
/// - missing / empty / `auto` → `None` (auto fallback chain)
/// - known alias → `Some(Engine)`
/// - unknown string → error (must not silently become auto)
fn parse_engine_arg(args: &Value) -> anyhow::Result<Option<Engine>> {
    let Some(raw) = args.get("engine").and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    Engine::from_str_opt(s).map(Some).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown engine `{s}`; use auto, duckduckgo, ddg, brave, exa, firecrawl, jina, perplexity, tavily, or serpapi"
        )
    })
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

    let preferred = parse_engine_arg(&args)?;

    if let Some(pref) = preferred
        && !pref.is_available()
        && let Some(env_var) = pref.key_env()
    {
        return Err(anyhow::anyhow!(
            "{} requires {} (engine was requested explicitly; not falling back to another provider)",
            pref.name(),
            env_var
        ));
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
    // Explicit engine: only that provider (no silent switch to Exa/etc).
    // Auto: ranked list with keyed engines first and DuckDuckGo last.
    let engines = ordered_try_list(preferred);
    let explicit = preferred.is_some();
    let mut errors = Vec::new();

    for engine in &engines {
        let api_key = engine.api_key();
        match search_engine(client, *engine, query, &api_key).await {
            Ok(results) if !results.is_empty() => {
                let limited = if results.len() > limit {
                    results[..limit].to_vec()
                } else {
                    results
                };
                return Ok((*engine, limited));
            }
            Ok(_) => errors.push(format!("{}: no results", engine.name())),
            Err(error) => errors.push(format!("{}: {error}", engine.name())),
        }
        // Explicit request: do not try other providers after a failure/empty set.
        if explicit {
            break;
        }
    }

    // Auto mode only: extra astral-tl HTML path when engine backends already failed.
    // (DuckDuckGo engine module already tried HTML/lite; this is a second parser path.)
    if !explicit {
        match search_duckduckgo(query).await {
            Ok(results) if !results.is_empty() => {
                let limited = if results.len() > limit {
                    results[..limit].to_vec()
                } else {
                    results
                };
                return Ok((Engine::DuckDuckGo, limited));
            }
            Ok(_) => errors.push("DuckDuckGo HTML fallback: no results".into()),
            Err(error) => errors.push(format!("DuckDuckGo HTML fallback: {error}")),
        }
    }

    if explicit {
        let name = preferred.map(|e| e.name()).unwrap_or("engine");
        return Err(anyhow::anyhow!(
            "web_search failed for requested engine {name} (no fallback to other engines): {}",
            errors.join("; ")
        ));
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

    #[test]
    fn parse_engine_arg_auto_and_aliases() {
        assert_eq!(parse_engine_arg(&json!({})).unwrap(), None);
        assert_eq!(parse_engine_arg(&json!({"engine": "auto"})).unwrap(), None);
        assert_eq!(parse_engine_arg(&json!({"engine": ""})).unwrap(), None);
        assert_eq!(
            parse_engine_arg(&json!({"engine": "duckduckgo"})).unwrap(),
            Some(Engine::DuckDuckGo)
        );
        assert_eq!(parse_engine_arg(&json!({"engine": "ddg"})).unwrap(), Some(Engine::DuckDuckGo));
        assert_eq!(parse_engine_arg(&json!({"engine": "exa"})).unwrap(), Some(Engine::Exa));
    }

    #[test]
    fn parse_engine_arg_rejects_unknown() {
        let err = parse_engine_arg(&json!({"engine": "not-a-real-engine"})).unwrap_err();
        assert!(err.to_string().contains("unknown engine"));
    }

    #[test]
    fn ordered_try_list_explicit_is_single_engine() {
        let list = ordered_try_list(Some(Engine::DuckDuckGo));
        assert_eq!(list, vec![Engine::DuckDuckGo]);

        // Prefer Jina only — must not prepend other engines when explicit.
        let list = ordered_try_list(Some(Engine::Jina));
        assert_eq!(list.first().copied(), Some(Engine::Jina));
        // Auto mode still multi-engine (Jina preferred first when available).
        // With the new strict semantics, explicit = only that engine.
        assert_eq!(list, vec![Engine::Jina]);
    }
}
