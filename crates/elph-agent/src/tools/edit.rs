//! Edit tool — ported from pi coding-agent `tools/edit.ts`.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::harness::types::{ExecutionEnv, Result as HarnessResult};
use crate::tools::common::{check_aborted, file_error, read_file_text, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_edit_tool(env: Arc<dyn ExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "edit".into(),
            description: "Replace text in a file. The old_string must match exactly once.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file to edit" },
                    "old_string": { "type": "string", "description": "Text to replace" },
                    "new_string": { "type": "string", "description": "Replacement text" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        "edit",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_edit(env, args, None).await })
        },
    )
}

async fn execute_edit(
    env: Arc<dyn ExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: path"))?;
    let old_string = args
        .get("old_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: old_string"))?;
    let new_string = args
        .get("new_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: new_string"))?;

    let absolute = resolve_path(&env, path, signal.as_ref()).await?;
    let content = read_file_text(&env, &absolute, signal.as_ref()).await?;
    let count = content.matches(old_string).count();
    if count == 0 {
        return Err(anyhow::anyhow!("old_string not found in {path}"));
    }
    if count > 1 {
        return Err(anyhow::anyhow!(
            "old_string found {count} times in {path}; must be unique"
        ));
    }
    let updated = content.replacen(old_string, new_string, 1);
    match env.write_file(&absolute, updated.as_bytes(), signal.as_ref()).await {
        HarnessResult::Ok(()) => Ok(AgentToolResult::text(format!("Edited {path}"))),
        HarnessResult::Err(error) => Err(file_error(error)),
    }
}
