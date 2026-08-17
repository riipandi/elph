//! Find tool — elph coding-agent tools.
//!
//! Uses fff-search with cached picker for gitignore-aware file discovery.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::utils::truncate::DEFAULT_MAX_BYTES;
use crate::agent::harness::utils::truncate::TruncationOptions;
use crate::agent::harness::utils::truncate::truncate_head;
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, resolve_path};
use crate::tools::fff_picker::{build_find_glob_pattern, build_find_options, cached_picker, run_with_abort_signal};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

const DEFAULT_LIMIT: usize = 200;

pub fn create_find_path_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "find_path".into(),
            constrained_sampling: None,

            description:
                "Find files by glob pattern (fast, respects .gitignore). Use '*.rs' or '**/*.rs' for recursive. \
Faster than grep for names/extensions. Prefer over shell find/ls. Use list_dir only for a known directory listing."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern, e.g. '*.rs', '**/todo_progress.rs', 'src/**/*.toml'" },
                    "path": { "type": "string", "description": "Directory to search in (default: workspace root)" },
                    "limit": { "type": "number", "description": "Max paths to return (default: 200)" }
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

    // Use cached fff picker for file discovery
    let signal_for_blocking = signal.clone();
    let base_for_thread = base.clone();
    let glob_for_thread = glob_pattern.clone();

    let (results, limit_reached) = tokio::task::spawn_blocking(move || {
        run_with_abort_signal(signal_for_blocking.as_ref(), |abort| {
            if abort.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(anyhow::anyhow!("Operation aborted"));
            }
            let picker = cached_picker(&base_for_thread)?;
            let search = picker.glob(&glob_for_thread, build_find_options(limit));
            let mut results: Vec<String> = search
                .items
                .iter()
                .map(|item| item.relative_path(picker.as_ref()))
                .collect();
            results.sort();
            let limit_reached = results.len() >= limit || search.total_matched > limit;
            if results.len() > limit {
                results.truncate(limit);
            }
            Ok((results, limit_reached))
        })
    })
    .await??;

    Ok(finish_find(results, limit_reached, limit))
}

fn finish_find(results: Vec<String>, limit_reached: bool, limit: usize) -> AgentToolResult {
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

    AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "resultLimitReached": limit_reached,
            "truncated": truncation.truncated
        }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    }
}
