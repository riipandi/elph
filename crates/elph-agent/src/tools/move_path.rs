//! Move path tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, ensure_parent_dir, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_move_path_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "move_path".into(),
            description: "Moves or renames a file or directory in the project, performing a rename if only the filename differs.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Source path to move from" },
                    "destination": { "type": "string", "description": "Destination path to move to" }
                },
                "required": ["source", "destination"]
            }),
        },
        "move_path",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_move_path(env, args, None).await })
        },
    )
}

async fn execute_move_path(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: source"))?;
    let destination = args
        .get("destination")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: destination"))?;

    let src_absolute = resolve_path(&env, source, signal.as_ref()).await?;
    let dst_absolute = resolve_path(&env, destination, signal.as_ref()).await?;

    if !std::path::Path::new(&src_absolute).exists() {
        return Err(anyhow::anyhow!("Source path does not exist: {source}"));
    }

    ensure_parent_dir(&env, &dst_absolute, signal.as_ref()).await?;

    tokio::fs::rename(&src_absolute, &dst_absolute)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to move path: {e}"))?;

    Ok(AgentToolResult::text(format!("Moved {source} to {destination}")))
}
