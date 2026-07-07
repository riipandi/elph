//! Grep tool — ported from pi coding-agent `tools/grep.ts`.

use std::sync::Arc;

use elph_ai::Tool;
use regex::Regex;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::harness::types::{ExecutionEnv, FileKind, Result as HarnessResult};
use crate::harness::utils::truncate::{
    DEFAULT_MAX_BYTES, GREP_MAX_LINE_LENGTH, TruncationOptions, truncate_head, truncate_line,
};
use crate::tools::common::{check_aborted, resolve_path};
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
    let regex = if literal {
        Regex::new(&regex::escape(pattern))?
    } else {
        let mut builder = regex::RegexBuilder::new(pattern);
        builder.case_insensitive(ignore_case);
        builder.build()?
    };

    let mut matches = Vec::new();
    let mut lines_truncated = false;
    collect_matches(
        &env,
        &absolute,
        &regex,
        &mut matches,
        &mut lines_truncated,
        limit,
        signal.as_ref(),
    )
    .await?;

    let output = matches.join("\n");
    let truncation = truncate_head(
        &output,
        TruncationOptions {
            max_bytes: Some(DEFAULT_MAX_BYTES),
            max_lines: None,
        },
    );
    let mut text = truncation.content;
    if matches.len() >= limit {
        text.push_str(&format!("\n\n[{limit} matches limit]"));
    }
    if truncation.truncated {
        text.push_str("\n\n[output truncated]");
    }

    Ok(AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "matchLimitReached": matches.len() >= limit,
            "linesTruncated": lines_truncated,
            "truncated": truncation.truncated
        }),
        terminate: None,
    })
}

async fn collect_matches(
    env: &Arc<dyn ExecutionEnv>,
    path: &str,
    regex: &Regex,
    matches: &mut Vec<String>,
    lines_truncated: &mut bool,
    limit: usize,
    signal: Option<&CancellationToken>,
) -> anyhow::Result<()> {
    if matches.len() >= limit {
        return Ok(());
    }
    check_aborted(signal)?;
    let info = match env.file_info(path, signal).await {
        HarnessResult::Ok(info) => info,
        HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
    };
    if info.kind == FileKind::File {
        let content = match env.read_text_file(path, signal).await {
            HarnessResult::Ok(content) => content,
            HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
        };
        for (index, line) in content.lines().enumerate() {
            if matches.len() >= limit {
                break;
            }
            if regex.is_match(line) {
                let (rendered, truncated) = truncate_line(line, GREP_MAX_LINE_LENGTH);
                if truncated {
                    *lines_truncated = true;
                }
                matches.push(format!("{}:{}:{}", path, index + 1, rendered));
            }
        }
        return Ok(());
    }
    if info.kind != FileKind::Directory {
        return Ok(());
    }
    let entries = match env.list_dir(path, signal).await {
        HarnessResult::Ok(entries) => entries,
        HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
    };
    let mut sorted = entries;
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for entry in sorted {
        if entry.name == ".git" || entry.name == "node_modules" || entry.name == "target" {
            continue;
        }
        Box::pin(collect_matches(
            env,
            &entry.path,
            regex,
            matches,
            lines_truncated,
            limit,
            signal,
        ))
        .await?;
        if matches.len() >= limit {
            break;
        }
    }
    Ok(())
}
