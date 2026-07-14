//! Write tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::{FileSystem, Result as HarnessResult};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, ensure_parent_dir, file_error, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_write_file_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "write_file".into(),
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
            Box::pin(async move { execute_write(env, args, None).await })
        },
    )
}

async fn execute_write(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
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
    ensure_parent_dir(&env, &absolute, signal.as_ref()).await?;
    match FileSystem::write_file(env.as_ref(), &absolute, content.as_bytes(), signal.as_ref()).await {
        HarnessResult::Ok(()) => Ok(AgentToolResult::text(format!("Wrote {} bytes to {path}", content.len()))),
        HarnessResult::Err(error) => Err(file_error(error)),
    }
}
