//! Grep tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::harness::types::{ExecutionEnv, FileKind, Result as HarnessResult};
use crate::harness::utils::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, truncate_head};
use crate::tools::common::{check_aborted, resolve_path};
use crate::tools::fff_picker::{
    build_grep_mode, build_grep_options, build_grep_query, build_picker, format_grep_output, parse_grep_query,
    resolve_path_scope, resolve_search_base, run_with_abort_signal,
};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

const DEFAULT_LIMIT: usize = 100;

pub fn create_grep_tool(env: Arc<dyn ExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "grep".into(),
            description: "Search for a regex pattern in files under a directory.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Search pattern (regex or literal string)" },
                    "path": { "type": "string", "description": "Directory or file to search" },
                    "ignoreCase": { "type": "boolean" },
                    "literal": { "type": "boolean" },
                    "limit": { "type": "number" }
                },
                "required": ["pattern"]
            }),
        },
        "grep",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_grep(env, args, None).await })
        },
    )
}

async fn execute_grep(
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
    let ignore_case = args.get("ignoreCase").and_then(|v| v.as_bool()).unwrap_or(false);
    let literal = args.get("literal").and_then(|v| v.as_bool()).unwrap_or(false);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_LIMIT as u64) as usize;

    let absolute = resolve_path(&env, path, signal.as_ref()).await?;
    let info = match env.file_info(&absolute, signal.as_ref()).await {
        HarnessResult::Ok(info) => info,
        HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
    };
    let is_file = info.kind == FileKind::File;
    if info.kind != FileKind::File && info.kind != FileKind::Directory {
        return Ok(AgentToolResult {
            content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(
                String::new(),
            ))],
            details: json!({
                "matchLimitReached": false,
                "linesTruncated": false,
                "truncated": false
            }),
            terminate: None,
        });
    }

    let base_path = resolve_search_base(&absolute, is_file);
    let path_scope = resolve_path_scope(&absolute, is_file);
    let (grep_pattern, mode) = build_grep_mode(pattern, literal, ignore_case);
    let query_text = build_grep_query(&grep_pattern, &path_scope);
    let signal_for_blocking = signal.clone();

    let (matches, lines_truncated, limit_reached) = tokio::task::spawn_blocking(move || {
        run_with_abort_signal(signal_for_blocking.as_ref(), |abort| {
            let parsed_query = parse_grep_query(&query_text);
            let picker = build_picker(&base_path)?;
            let options = build_grep_options(limit, mode, ignore_case, abort);
            let result = picker.grep(&parsed_query, &options);
            let (matches, lines_truncated) = format_grep_output(&picker, &result);
            Ok((matches, lines_truncated, result.matches.len() >= limit))
        })
    })
    .await??;

    let output = matches.join("\n");
    let truncation = truncate_head(
        &output,
        TruncationOptions {
            max_bytes: Some(DEFAULT_MAX_BYTES),
            max_lines: None,
        },
    );
    let mut text = truncation.content;
    if limit_reached {
        text.push_str(&format!("\n\n[{limit} matches limit]"));
    }
    if truncation.truncated {
        text.push_str("\n\n[output truncated]");
    }

    Ok(AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "matchLimitReached": limit_reached,
            "linesTruncated": lines_truncated,
            "truncated": truncation.truncated
        }),
        terminate: None,
    })
}
