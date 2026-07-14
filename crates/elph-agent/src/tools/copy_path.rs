//! Copy path tool — elph coding-agent tools.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, ensure_parent_dir, resolve_path};
use crate::tools::fff_picker::run_with_abort_signal;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_copy_path_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "copy_path".into(),
            description: "Copies a file or directory recursively in the project, more efficient than manually reading and writing files when duplicating content.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Source path to copy from" },
                    "destination": { "type": "string", "description": "Destination path to copy to" }
                },
                "required": ["source", "destination"]
            }),
        },
        "copy_path",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_copy_path(env, args, None).await })
        },
    )
}

async fn execute_copy_path(
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

    let src_path = Path::new(&src_absolute);
    if !src_path.exists() {
        return Err(anyhow::anyhow!("Source path does not exist: {source}"));
    }

    ensure_parent_dir(&env, &dst_absolute, signal.as_ref()).await?;

    if src_path.is_file() {
        tokio::fs::copy(&src_absolute, &dst_absolute)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to copy file: {e}"))?;
        Ok(AgentToolResult::text(format!("Copied {source} to {destination}")))
    } else {
        // Directory copy — walk and copy recursively
        let signal_for_blocking = signal.clone();
        let src = src_absolute.clone();
        let dst = dst_absolute.clone();
        tokio::task::spawn_blocking(move || {
            run_with_abort_signal(signal_for_blocking.as_ref(), |abort| {
                copy_directory_recursive(&src, &dst, &abort)
            })
        })
        .await??;
        Ok(AgentToolResult::text(format!("Copied directory {source} to {destination}")))
    }
}

fn copy_directory_recursive(src: &str, dst: &str, abort: &AtomicBool) -> anyhow::Result<()> {
    for entry in WalkDir::new(src).min_depth(1) {
        if abort.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("Operation aborted"));
        }
        let entry = entry?;
        let relative = entry.path().strip_prefix(src).map_err(|e| anyhow::anyhow!("{e}"))?;
        let target = Path::new(dst).join(relative);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
