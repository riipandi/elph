//! Create directory tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::{CreateDirOptions, FileSystem, Result as HarnessResult};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, file_error, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};
use crate::workers::SharedPathClaim;

pub fn create_create_dir_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    create_create_dir_tool_with_claims(env, None)
}

pub fn create_create_dir_tool_with_claims(env: Arc<LocalExecutionEnv>, claims: SharedPathClaim) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "create_dir".into(),
            constrained_sampling: None,
            description: "Creates a new directory at the specified path, creating all necessary parent directories (similar to `mkdir -p`).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to create" }
                },
                "required": ["path"]
            }),
        },
        "create_dir",
        move |_, args| {
            let env = env_for_tool.clone();
            let claims = claims.clone();
            Box::pin(async move { execute_create_dir(env, args, None, claims).await })
        },
    )
}

async fn execute_create_dir(
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

    let absolute = resolve_path(&env, path, signal.as_ref()).await?;
    if let Some(claim) = claims.as_ref() {
        claim.claim(&absolute, "create_dir").await?;
    }
    match FileSystem::create_dir(
        env.as_ref(),
        &absolute,
        Some(CreateDirOptions {
            recursive: true,
            abort_token: signal,
        }),
    )
    .await
    {
        HarnessResult::Ok(()) => Ok(AgentToolResult::text(format!("Created directory {path}"))),
        HarnessResult::Err(error) => Err(file_error(error)),
    }
}
