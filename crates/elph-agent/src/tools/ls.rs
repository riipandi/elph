//! List directory tool — elph coding-agent tools.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

use crate::env::LocalExecutionEnv;
use crate::harness::types::{FileKind, FileSystem, Result as HarnessResult};
use crate::harness::utils::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, truncate_head};
use crate::tools::common::{check_aborted, resolve_path};
use crate::tools::fff_picker::run_with_abort_signal;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

const DEFAULT_LIMIT: usize = 1000;

pub fn create_ls_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
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
    env: Arc<LocalExecutionEnv>,
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

    let signal_for_blocking = signal.clone();
    let names = tokio::task::spawn_blocking(move || {
        run_with_abort_signal(signal_for_blocking.as_ref(), |abort| {
            list_directory(&absolute, limit, &abort)
        })
    })
    .await??;

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

fn list_directory(path: &str, limit: usize, abort: &AtomicBool) -> anyhow::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in WalkDir::new(path).min_depth(1).max_depth(1) {
        if abort.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("Operation aborted"));
        }
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().is_dir() {
            names.push(format!("{file_name}/"));
        } else {
            names.push(file_name);
        }
    }
    names.sort_by_key(|name| name.to_lowercase());
    if names.len() > limit {
        names.truncate(limit);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn list_directory_sorts_and_suffixes_dirs() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("b.txt"), "").unwrap();
        fs::create_dir(dir.path().join("A-dir")).unwrap();

        let names = list_directory(&dir.path().to_string_lossy(), DEFAULT_LIMIT, &AtomicBool::new(false)).unwrap();

        assert_eq!(names, vec!["A-dir/".to_string(), "b.txt".to_string()]);
    }

    #[test]
    fn list_directory_respects_limit() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        fs::write(dir.path().join("b.txt"), "").unwrap();

        let names = list_directory(&dir.path().to_string_lossy(), 1, &AtomicBool::new(false)).unwrap();
        assert_eq!(names.len(), 1);
    }
}
