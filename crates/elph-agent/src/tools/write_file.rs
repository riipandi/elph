//! Write tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::{FileSystem, Result as HarnessResult};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, ensure_parent_dir, file_error, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};
use crate::workers::SharedPathClaim;

pub fn create_write_file_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    create_write_file_tool_with_claims(env, None)
}

pub fn create_write_file_tool_with_claims(env: Arc<LocalExecutionEnv>, claims: SharedPathClaim) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "write_file".into(),
            constrained_sampling: None,
            description: "Creates a new file or overwrites an existing file with completely new contents. Creates parent directories when needed.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file to write" },
                    "content": { "type": "string", "description": "Content to write to the file" }
                },
                "required": ["path", "content"]
            }),
        },
        "write_file",
        move |_, args| {
            let env = env_for_tool.clone();
            let claims = claims.clone();
            Box::pin(async move { execute_write(env, args, None, claims).await })
        },
    )
}

async fn execute_write(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
    claims: SharedPathClaim,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: path"))?;
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: content"))?;

    let absolute = resolve_path(&env, path, signal.as_ref()).await?;
    if let Some(claim) = claims.as_ref() {
        claim.claim(&absolute, "write_file").await?;
    }
    ensure_parent_dir(&env, &absolute, signal.as_ref()).await?;

    // Read existing content before writing (for diff display)
    let old_content = match env.read_text_file(&absolute, signal.as_ref()).await {
        HarnessResult::Ok(content) => content,
        HarnessResult::Err(_) => String::new(), // File doesn't exist or can't be read
    };

    match FileSystem::write_file(env.as_ref(), &absolute, content.as_bytes(), signal.as_ref()).await {
        HarnessResult::Ok(()) => Ok(AgentToolResult {
            content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(
                format!("Wrote {} bytes to {path}", content.len()),
            ))],
            details: json!({
                "old_content": old_content,
                "new_content": content,
                "file_path": absolute,
            }),
            added_tool_names: None,
            terminate: None,
            usage: None,
        }),
        HarnessResult::Err(error) => Err(file_error(error)),
    }
}
