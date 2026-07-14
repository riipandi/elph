//! Find tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::agent::harness::utils::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, truncate_head};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, resolve_path};
use crate::tools::fff_picker::{build_find_glob_pattern, build_find_options, build_picker, run_with_abort_signal};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

const DEFAULT_LIMIT: usize = 1000;

pub fn create_find_path_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "find_path".into(),
            description: "Quickly finds files by matching glob patterns (like '*.rs'), returning matching file paths alphabetically.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern, e.g. '*.rs'" },
                    "path": { "type": "string", "description": "Directory to search in" },
                    "limit": { "type": "number" }
                },
                "required": ["pattern"]
            }),
        },
        "find_path",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_find_path(env, args, None).await })
        },
    )
}

async fn execute_find_path(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let pattern = args
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: pattern"))?;
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_LIMIT as u64) as usize;

    let base = resolve_path(&env, path, signal.as_ref()).await?;
    let glob_pattern = build_find_glob_pattern(pattern);
    let signal_for_blocking = signal.clone();

    let (results, limit_reached) = tokio::task::spawn_blocking(move || {
        run_with_abort_signal(signal_for_blocking.as_ref(), |abort| {
            if abort.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(anyhow::anyhow!("Operation aborted"));
            }
            let picker = build_picker(&base)?;
            let search = picker.glob(&glob_pattern, build_find_options(limit));
            let mut results: Vec<String> = search.items.iter().map(|item| item.relative_path(&picker)).collect();
            results.sort();
            let limit_reached = results.len() >= limit || search.total_matched > limit;
            if results.len() > limit {
                results.truncate(limit);
            }
            Ok((results, limit_reached))
        })
    })
    .await??;

    let output = results.join("\n");
    let truncation = truncate_head(
        &output,
        TruncationOptions {
            max_bytes: Some(DEFAULT_MAX_BYTES),
            max_lines: None,
        },
    );
    let mut text = truncation.content;
    if limit_reached {
        text.push_str(&format!("\n\n[{limit} results limit]"));
    }
    if truncation.truncated {
        text.push_str("\n\n[output truncated]");
    }

    Ok(AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "resultLimitReached": limit_reached,
            "truncated": truncation.truncated
        }),
        added_tool_names: None,
        terminate: None,
    })
}
