//! List directory tool — ported from pi coding-agent `tools/ls.ts`.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::harness::types::{ExecutionEnv, FileKind, Result as HarnessResult};
use crate::harness::utils::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, truncate_head};
use crate::tools::common::{check_aborted, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

const DEFAULT_LIMIT: usize = 1000;

pub fn create_ls_tool(env: Arc<dyn ExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "ls".into(),
            description: "List directory contents.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path" },
                    "limit": { "type": "number" }
                }
            }),
        },
        "ls",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_ls(env, args, None).await })
        },
    )
}

async fn execute_ls(
    env: Arc<dyn ExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_LIMIT as u64) as usize;
    let absolute = resolve_path(&env, path, signal.as_ref()).await?;
    let info = match env.file_info(&absolute, signal.as_ref()).await {
        HarnessResult::Ok(info) => info,
        HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
    };
    if info.kind != FileKind::Directory {
        return Err(anyhow::anyhow!("Not a directory: {path}"));
    }
    let entries = match env.list_dir(&absolute, signal.as_ref()).await {
        HarnessResult::Ok(entries) => entries,
        HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
    };
    let mut names: Vec<String> = entries
        .into_iter()
        .map(|entry| {
            if entry.kind == FileKind::Directory {
                format!("{}/", entry.name)
            } else {
                entry.name
            }
        })
        .collect();
    names.sort_by_key(|a| a.to_lowercase());
    if names.len() > limit {
        names.truncate(limit);
    }
    let output = names.join("\n");
    let truncation = truncate_head(
        &output,
        TruncationOptions {
            max_bytes: Some(DEFAULT_MAX_BYTES),
            max_lines: None,
        },
    );
    let mut text = truncation.content;
    if truncation.truncated {
        text.push_str("\n\n[output truncated]");
    }
    Ok(AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({ "truncated": truncation.truncated }),
        terminate: None,
    })
}
