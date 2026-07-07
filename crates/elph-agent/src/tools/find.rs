//! Find tool — ported from pi coding-agent `tools/find.ts`.

use std::sync::Arc;

use elph_ai::Tool;
use glob::glob;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::harness::types::ExecutionEnv;
use crate::harness::utils::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, truncate_head};
use crate::tools::common::{check_aborted, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

const DEFAULT_LIMIT: usize = 1000;

pub fn create_find_tool(env: Arc<dyn ExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "find".into(),
            description: "Search for files by glob pattern.".into(),
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
        "find",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_find(env, args, None).await })
        },
    )
}

async fn execute_find(
    env: Arc<dyn ExecutionEnv>,
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
    let glob_pattern = if pattern.contains('/') {
        format!("{base}/{pattern}")
    } else {
        format!("{base}/**/{pattern}")
    };

    let mut results = Vec::new();
    for entry in glob(&glob_pattern).map_err(|error| anyhow::anyhow!("{error}"))? {
        check_aborted(signal.as_ref())?;
        let entry = entry.map_err(|error| anyhow::anyhow!("{error}"))?;
        let display = entry.to_string_lossy().replace('\\', "/");
        let relative = display
            .strip_prefix(&format!("{base}/"))
            .unwrap_or(&display)
            .to_string();
        results.push(relative);
        if results.len() >= limit {
            break;
        }
    }
    results.sort();

    let output = results.join("\n");
    let truncation = truncate_head(
        &output,
        TruncationOptions {
            max_bytes: Some(DEFAULT_MAX_BYTES),
            max_lines: None,
        },
    );
    let mut text = truncation.content;
    if results.len() >= limit {
        text.push_str(&format!("\n\n[{limit} results limit]"));
    }
    if truncation.truncated {
        text.push_str("\n\n[output truncated]");
    }

    Ok(AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "resultLimitReached": results.len() >= limit,
            "truncated": truncation.truncated
        }),
        terminate: None,
    })
}
