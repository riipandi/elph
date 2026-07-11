//! Read tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::env::LocalExecutionEnv;
use crate::harness::utils::truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncationOptions, format_size, truncate_head,
};
use crate::tools::common::{check_aborted, is_probably_image, read_file_text, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_read_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "read".into(),
            description: format!(
                "Read the contents of a file. Output is truncated to {DEFAULT_MAX_LINES} lines or {}/KB.",
                DEFAULT_MAX_BYTES / 1024
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file to read (relative or absolute)" },
                    "offset": { "type": "number", "description": "Line number to start reading from (1-indexed)" },
                    "limit": { "type": "number", "description": "Maximum number of lines to read" }
                },
                "required": ["path"]
            }),
        },
        "read",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_read(env, args, None).await })
        },
    )
}

async fn execute_read(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: path"))?;
    let offset = args.get("offset").and_then(|v| v.as_u64()).map(|v| v as usize);
    let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);

    let absolute = resolve_path(&env, path, signal.as_ref()).await?;
    if is_probably_image(&absolute) {
        return Ok(AgentToolResult::text(format!(
            "Read image file [{absolute}] (image content omitted)"
        )));
    }

    let content = read_file_text(&env, &absolute, signal.as_ref()).await?;
    let start_line = offset.map(|value| value.saturating_sub(1)).unwrap_or(0);
    let selected = match crate::harness::utils::truncate::select_line_range(&content, start_line, limit) {
        Ok(selected) => selected,
        Err(total_lines) => {
            return Err(anyhow::anyhow!(
                "Offset {} is beyond end of file ({} lines total)",
                offset.unwrap_or(1),
                total_lines
            ));
        }
    };

    let truncation = truncate_head(&selected, TruncationOptions::default());
    let mut output = truncation.content;
    if truncation.first_line_exceeds_limit {
        output = format!(
            "[Line {} exceeds {} limit. Use bash to read a portion of the file.]",
            start_line + 1,
            format_size(DEFAULT_MAX_BYTES)
        );
    } else if truncation.truncated {
        output.push_str(&format!(
            "\n\n[Truncated: showing first {} lines / {}]",
            truncation.output_lines,
            format_size(truncation.output_bytes)
        ));
    }

    Ok(AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(output))],
        details: json!({ "truncation": truncation.truncated }),
        added_tool_names: None,
        terminate: None,
    })
}
