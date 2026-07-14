//! Delete path tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::{FileSystem, RemoveOptions, Result as HarnessResult};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, file_error, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_delete_path_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "delete_path".into(),
            description: "Deletes a file or directory (including contents recursively) at the specified path and confirms the deletion.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file or directory to delete" }
                },
                "required": ["path"]
            }),
        },
        "delete_path",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_delete_path(env, args, None).await })
        },
    )
}

async fn execute_delete_path(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: path"))?;

    let absolute = resolve_path(&env, path, signal.as_ref()).await?;
    match FileSystem::remove(
        env.as_ref(),
        &absolute,
        Some(RemoveOptions {
            recursive: true,
            force: false,
            abort_token: signal,
        }),
    )
    .await
    {
        HarnessResult::Ok(()) => Ok(AgentToolResult::text(format!("Deleted {path}"))),
        HarnessResult::Err(error) => Err(file_error(error)),
    }
}
