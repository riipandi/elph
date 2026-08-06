//! User-initiated shell commands (`!` / `!!`) from the prompt editor.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use elph_agent::agent::harness::types::Result as HarnessResult;
use elph_agent::{ExecutionErrorCode, FileSystem, LocalExecutionEnv, Shell, ShellExecOptions, finalize_shell_capture};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

static SHELL_TOOL_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Events from an in-flight user shell command, polled by the shell tick loop.
#[derive(Debug, Clone)]
pub enum UserShellEvent {
    ToolUpdate {
        id: String,
        chunk: String,
    },
    ToolEnd {
        id: String,
        exit_code: Option<i32>,
        output: String,
        cancelled: bool,
        with_context: bool,
        command: String,
    },
}

/// Allocate a stable tool-card id for a user shell invocation.
pub fn next_user_shell_tool_id() -> String {
    format!("user-shell-{}", SHELL_TOOL_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Spawn a shell command triggered from the prompt (`!` or `!!`).
pub fn spawn_user_shell(
    env: Arc<LocalExecutionEnv>,
    tool_id: String,
    command: String,
    with_context: bool,
    abort_token: CancellationToken,
    event_tx: UnboundedSender<UserShellEvent>,
) {
    let id_for_task = tool_id;
    let command_for_task = command.clone();

    tokio::spawn(async move {
        let streamed = Arc::new(Mutex::new(String::new()));
        let chunk_tx = event_tx.clone();
        let chunk_id = id_for_task.clone();
        let streamed_capture = streamed.clone();
        let on_chunk = Arc::new(move |chunk: &str| {
            if let Ok(mut buffer) = streamed_capture.lock() {
                buffer.push_str(chunk);
            }
            let _ = chunk_tx.send(UserShellEvent::ToolUpdate {
                id: chunk_id.clone(),
                chunk: chunk.to_string(),
            });
        });

        let cwd = env.cwd().to_string();
        let exec_result = env
            .exec(
                &command_for_task,
                Some(ShellExecOptions {
                    cwd: Some(cwd),
                    env: None,
                    timeout: None,
                    abort_token: Some(abort_token),
                    on_stdout: Some(on_chunk.clone()),
                    on_stderr: Some(on_chunk),
                    ..Default::default()
                }),
            )
            .await;

        let (exit_code, output, cancelled) = match exec_result {
            HarnessResult::Ok(result) => {
                let mut combined = result.stdout;
                if !result.stderr.is_empty() {
                    if !combined.is_empty() && !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                    combined.push_str(&result.stderr);
                }
                let streamed_text = streamed.lock().map(|buffer| buffer.clone()).unwrap_or_default();
                let finalized =
                    finalize_shell_capture(if combined.is_empty() { &streamed_text } else { &combined }, None);
                (Some(result.exit_code), finalized.output, false)
            }
            HarnessResult::Err(error) => {
                let cancelled = error.code == ExecutionErrorCode::Aborted;
                (
                    if cancelled { None } else { Some(1) },
                    if let Ok(buffer) = streamed.lock() {
                        if buffer.is_empty() {
                            error.message.clone()
                        } else {
                            buffer.clone()
                        }
                    } else {
                        error.message.clone()
                    },
                    cancelled,
                )
            }
        };

        let _ = event_tx.send(UserShellEvent::ToolEnd {
            id: id_for_task,
            exit_code,
            output,
            cancelled,
            with_context,
            command: command_for_task,
        });
    });
}

pub fn format_shell_agent_context(command: &str, output: &str) -> String {
    if output.trim().is_empty() {
        format!("Output of `$ {command}`:\n\n(empty)")
    } else {
        format!("Output of `$ {command}`:\n\n{output}")
    }
}

pub fn shell_exec_args_summary(command: &str) -> String {
    serde_json::json!({ "command": command }).to_string()
}
