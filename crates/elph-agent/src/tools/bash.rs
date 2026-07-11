//! Bash tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::env::LocalExecutionEnv;
use crate::harness::types::{FileSystem, Shell, ShellExecOptions};
use crate::harness::utils::shell_output::finalize_shell_capture;
use crate::harness::utils::truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES};
use crate::tools::common::{check_aborted, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_bash_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "bash".into(),
            description: format!(
                "Execute a bash command in the current working directory. Output truncated to last {DEFAULT_MAX_LINES} lines or {}/KB.",
                DEFAULT_MAX_BYTES / 1024
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Bash command to execute" },
                    "timeout": { "type": "number", "description": "Timeout in seconds" }
                },
                "required": ["command"]
            }),
        },
        "bash",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_bash(env, args, None).await })
        },
    )
}

async fn execute_bash(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: command"))?;
    let timeout = args.get("timeout").and_then(|v| v.as_u64());

    let cwd = env.cwd().to_string();
    let _ = resolve_path(&env, ".", signal.as_ref()).await?;
    let result = env
        .exec(
            command,
            Some(ShellExecOptions {
                cwd: Some(cwd),
                env: None,
                timeout,
                abort_token: signal,
                on_stdout: None,
                on_stderr: None,
            }),
        )
        .await;

    let result = match result {
        crate::harness::types::Result::Ok(result) => result,
        crate::harness::types::Result::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
    };

    let mut combined = result.stdout;
    if !result.stderr.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&result.stderr);
    }
    let capture = finalize_shell_capture(&combined, None);
    let mut text = capture.output;
    if let Some(code) = Some(result.exit_code)
        && code != 0
    {
        text.push_str(&format!("\n\n[exit code: {code}]"));
    }
    if capture.truncated {
        text.push_str("\n\n[output truncated]");
    }

    Ok(AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({ "exitCode": result.exit_code, "truncated": capture.truncated }),
        added_tool_names: None,
        terminate: None,
    })
}
