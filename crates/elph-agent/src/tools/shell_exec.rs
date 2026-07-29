//! Shell execution tool — elph coding-agent tools.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::Tool;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::FileSystem;
use crate::agent::harness::types::Result as HarnessResult;
use crate::agent::harness::utils::shell_output::{ShellCaptureOptions, execute_shell_with_capture};
use crate::agent::harness::utils::truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, resolve_path};
use crate::types::{AgentTool, AgentToolResult, ToolExecuteFn, ToolResultContent, ToolUpdateCallback};
use elph_ai::TextContent;

pub fn create_shell_exec_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    AgentTool {
        tool: Tool {
            name: "shell_exec".into(),
            constrained_sampling: None,

            description: format!(
                "Execute a shell command in the current working directory. Output truncated to last {DEFAULT_MAX_LINES} lines or {}/KB.",
                DEFAULT_MAX_BYTES / 1024
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "timeout": { "type": "number", "description": "Timeout in seconds" }
                },
                "required": ["command"]
            }),
        },
        label: "shell_exec".into(),
        execution_mode: None,
        prepare_arguments: None,
        execute: shell_exec_execute_fn(env_for_tool),
    }
}

fn shell_exec_execute_fn(env: Arc<LocalExecutionEnv>) -> ToolExecuteFn {
    Arc::new(
        move |_id, args, signal, on_update| -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
            let env = env.clone();
            Box::pin(async move { execute_shell_exec(env, args, signal, on_update).await })
        },
    )
}

async fn execute_shell_exec(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
    on_update: Option<ToolUpdateCallback>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: command"))?;
    let timeout = args.get("timeout").and_then(|v| v.as_u64());

    let cwd = env.cwd().to_string();
    let _ = resolve_path(&env, ".", signal.as_ref()).await?;

    let on_progress = on_update.map(|callback| {
        Arc::new(move |chunk: &str| {
            callback(AgentToolResult {
                content: vec![ToolResultContent::Text(TextContent::new(chunk))],
                details: json!({ "streaming": true }),
                added_tool_names: None,
                terminate: None,
                usage: None,
            });
        }) as Arc<dyn Fn(&str) + Send + Sync>
    });

    let capture = match execute_shell_with_capture(
        env.as_ref(),
        command,
        Some(ShellCaptureOptions {
            cwd: Some(cwd),
            env: None,
            timeout,
            abort_token: signal,
            on_progress,
        }),
    )
    .await
    {
        HarnessResult::Ok(capture) => capture,
        HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
    };

    let mut text = capture.output;
    if let Some(code) = capture.exit_code
        && code != 0
    {
        text.push_str(&format!("\n\n[exit code: {code}]"));
    }
    if capture.truncated {
        if let Some(path) = &capture.full_output_path {
            text.push_str(&format!("\n\n[Output truncated. Full output: {path}]"));
        } else {
            text.push_str("\n\n[output truncated]");
        }
    }

    Ok(AgentToolResult {
        content: vec![ToolResultContent::Text(TextContent::new(text))],
        details: json!({
            "exitCode": capture.exit_code,
            "truncated": capture.truncated,
            "cancelled": capture.cancelled,
            "fullOutputPath": capture.full_output_path,
        }),
        added_tool_names: None,
        terminate: None,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::runtime::local_env::LocalExecutionEnv;
    use tempfile::TempDir;

    #[tokio::test]
    async fn shell_exec_tool_streams_output_before_completion() {
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let saw_early = Arc::new(AtomicBool::new(false));
        let saw_early_capture = saw_early.clone();
        let on_update: ToolUpdateCallback = Arc::new(move |partial| {
            let text = partial
                .content
                .iter()
                .filter_map(|block| match block {
                    ToolResultContent::Text(text) => Some(text.text.as_str()),
                    _ => None,
                })
                .collect::<String>();
            if text.contains("early") {
                saw_early_capture.store(true, Ordering::SeqCst);
            }
        });

        let result = execute_shell_exec(
            env,
            json!({ "command": "printf early; sleep 0.2; printf late", "timeout": 5 }),
            None,
            Some(on_update),
        )
        .await
        .expect("shell_exec execution");

        assert!(saw_early.load(Ordering::SeqCst));
        let text = match &result.content[0] {
            ToolResultContent::Text(text) => text.text.as_str(),
            _ => panic!("expected text result"),
        };
        assert!(text.contains("late"));
    }
}
