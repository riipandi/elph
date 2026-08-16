//! Shell execution tool — elph coding-agent tools.

use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use elph_ai::Tool;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::CreateTempFileOptions;
use crate::agent::harness::types::FileSystem;
use crate::agent::harness::types::Result as HarnessResult;
use crate::agent::harness::types::Shell;
use crate::agent::harness::types::ShellExecOptions;
use crate::agent::harness::utils::shell_output::{
    ShellCaptureOptions, ShellCaptureResult, ShellOutputCallback, execute_shell_with_capture,
};
use crate::agent::harness::utils::truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, resolve_path};
use crate::types::{AgentTool, AgentToolResult, ToolExecuteFn, ToolResultContent, ToolUpdateCallback};
use elph_ai::TextContent;

/// Max characters kept in a shell_exec tool result before it enters the agent
/// context. Matches the MCP bound so session message memory stays bounded.
const MAX_TOOL_RESULT_CHARS: usize = 32_768;

/// Default timeout (seconds) for background tasks in interactive (TUI) mode.
const BACKGROUND_DEFAULT_TIMEOUT_SECS: u64 = 600;

/// Create a shell_exec tool that uses ToolContext for execution.
///
/// Unlike the old pattern (capturing `Arc<LocalExecutionEnv>` at construction),
/// this tool receives the env from the harness-provided `ToolContext` at runtime.
pub fn create_shell_exec_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let cwd = env.cwd();
    AgentTool {
        tool: Tool {
            name: "shell_exec".into(),
            constrained_sampling: None,
            description: format!(
                "Execute a shell command in the current working directory: {cwd}. \
                 Commands already run in that directory — do NOT prefix them with `cd {cwd} &&` \
                 or any other `cd ... &&`. Output truncated to last {DEFAULT_MAX_LINES} lines or {}/KB. \
                 Set `run_in_background` to detach the command and return immediately with a task handle \
                 and an output file; background tasks default to a 10-minute timeout (no limit in headless \
                 `elph run`). `description` is required when `run_in_background` is true. `disable_timeout` \
                 removes the timeout limit for both foreground and background runs.",
                DEFAULT_MAX_BYTES / 1024
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute (runs in the working directory)" },
                    "timeout": { "type": "number", "description": "Timeout in seconds" },
                    "run_in_background": { "type": "boolean", "description": "Run as a background task; returns immediately with a task handle and an output file path" },
                    "disable_timeout": { "type": "boolean", "description": "Remove the timeout limit (foreground and background)" },
                    "description": { "type": "string", "description": "Background task description; required when run_in_background is true" }
                },
                "required": ["command"]
            }),
        },
        label: "shell_exec".into(),
        execution_mode: None,
        prepare_arguments: None,
        execute: shell_exec_execute_fn(),
    }
}

/// Strip a redundant leading `cd <cwd>` prefix from a shell command.
///
/// `shell_exec` already runs commands in the working directory, so a leading
/// `cd <cwd> && …` (or `;` / newline separator) written by the model is a no-op
/// that only clutters tool cards in the transcript. The prefix is removed only
/// when the `cd` target resolves lexically to the same directory as `cwd`
/// (trailing slashes, `.`/`..` components, and relative targets are handled).
/// Anything else — a different directory, `~`, no prefix, or `cd` as the entire
/// command — is returned unchanged.
pub fn strip_redundant_cd_prefix(command: &str, cwd: &str) -> String {
    let trimmed = command.trim_start();
    let Some(rest) = trimmed.strip_prefix("cd ") else {
        return command.to_string();
    };

    let (token, tail) = split_shell_word(rest);
    let Some(token) = token else {
        return command.to_string();
    };

    // A shell operator glued to the path (`cd /path;make`, `cd /path&&make`).
    let (path_word, glued) = if is_quoted_word(token) {
        (token, "")
    } else {
        match split_glued_operator(token) {
            Some((path, rest)) => (path, rest),
            None => (token, ""),
        }
    };
    let target = unquote_shell_word(path_word);
    if target.is_empty() || !cd_target_matches_cwd(&target, cwd) {
        return command.to_string();
    }

    // Skip the command separator (`&&`, `;`, or a newline) and whitespace.
    let combined = format!("{glued}{tail}");
    let combined = combined.trim_start();
    let remainder = combined
        .strip_prefix("&&")
        .or_else(|| combined.strip_prefix(';'))
        .map(str::trim_start)
        .unwrap_or(combined)
        .trim();
    if remainder.is_empty() {
        // `cd <cwd>` as the whole command — keep it rather than emit a no-op.
        return command.to_string();
    }
    remainder.to_string()
}

/// Split an unquoted token at the first shell operator (`&&` or `;`) it contains.
fn split_glued_operator(token: &str) -> Option<(&str, &str)> {
    for (idx, _) in token.char_indices() {
        let rest = &token[idx..];
        if let Some(sep) = rest.strip_prefix("&&").or_else(|| rest.strip_prefix(';')) {
            return Some((&token[..idx], sep));
        }
    }
    None
}

/// Whether a shell word starts with a quote (separators inside are literal).
fn is_quoted_word(word: &str) -> bool {
    word.chars().next().is_some_and(|c| matches!(c, '"' | '\''))
}

/// Normalize `shell_exec` tool arguments by stripping a redundant `cd <cwd>`
/// prefix from the `command` field. Returns the input unchanged when the args
/// are not a shell_exec-shaped object or nothing was stripped.
pub fn normalize_shell_exec_args(args_json: &str, cwd: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(args_json) else {
        return args_json.to_string();
    };
    let Some(command) = value.get("command").and_then(Value::as_str) else {
        return args_json.to_string();
    };
    let stripped = strip_redundant_cd_prefix(command, cwd);
    if stripped == command {
        return args_json.to_string();
    }
    value["command"] = Value::String(stripped);
    serde_json::to_string(&value).unwrap_or_else(|_| args_json.to_string())
}

/// Split the first shell word from the rest of the line, honoring quotes.
fn split_shell_word(input: &str) -> (Option<&str>, &str) {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return (None, trimmed);
    }
    if let Some(quote) = trimmed.chars().next().filter(|c| matches!(c, '"' | '\'')) {
        let mut escaped = false;
        for (idx, ch) in trimmed.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == quote {
                let end = idx + ch.len_utf8();
                return (Some(&trimmed[..end]), &trimmed[end..]);
            }
        }
        // Unterminated quote — do not guess.
        return (None, trimmed);
    }
    let end = trimmed
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(idx, _)| idx)
        .unwrap_or(trimmed.len());
    (Some(&trimmed[..end]), &trimmed[end..])
}

/// Strip matching surrounding quotes from a shell word.
fn unquote_shell_word(word: &str) -> String {
    let word = word.trim();
    let mut chars = word.chars();
    let (Some(first), Some(last)) = (chars.next(), chars.next_back()) else {
        return word.to_string();
    };
    if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
        return word[first.len_utf8()..word.len() - last.len_utf8()].to_string();
    }
    word.to_string()
}

/// Lexically compare a `cd` target with the working directory (no filesystem access).
fn cd_target_matches_cwd(target: &str, cwd: &str) -> bool {
    let Some(resolved) = resolve_cd_target(target, cwd) else {
        return false;
    };
    let Some(normalized_cwd) = normalize_lexical(cwd) else {
        return false;
    };
    resolved == normalized_cwd
}

/// Resolve a `cd` target to a normalized absolute path.
fn resolve_cd_target(target: &str, cwd: &str) -> Option<String> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    let absolute = if target.starts_with('/') {
        target.to_string()
    } else {
        format!("{}/{}", cwd.trim_end_matches('/'), target)
    };
    normalize_lexical(&absolute)
}

/// Collapse `.`/`..` components, duplicate slashes, and trailing slashes lexically.
fn normalize_lexical(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() || !path.starts_with('/') {
        return None;
    }
    let mut stack: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    Some(format!("/{}", stack.join("/")))
}

fn shell_exec_execute_fn() -> ToolExecuteFn {
    Arc::new(
        move |id,
              args,
              signal,
              on_update,
              context|
              -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
            let env = context.env.clone();
            Box::pin(async move { execute_shell_exec(env, id, args, signal, on_update, context).await })
        },
    )
}

async fn execute_shell_exec(
    env: Arc<LocalExecutionEnv>,
    id: String,
    args: Value,
    signal: Option<CancellationToken>,
    on_update: Option<ToolUpdateCallback>,
    context: crate::tools::types::ToolContext,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: command"))?;
    let run_in_background = args.get("run_in_background").and_then(Value::as_bool).unwrap_or(false);
    let disable_timeout = args.get("disable_timeout").and_then(Value::as_bool).unwrap_or(false);
    let explicit_timeout = args.get("timeout").and_then(Value::as_u64);
    let description = args.get("description").and_then(Value::as_str).map(str::to_string);

    let cwd = env.cwd().to_string();
    let _ = resolve_path(&env, ".", signal.as_ref()).await?;

    if run_in_background {
        let description = match description {
            Some(description) if !description.trim().is_empty() => description,
            _ => {
                return Err(anyhow::anyhow!("description is required when run_in_background is true"));
            }
        };
        // Resolve the background timeout: an explicit `timeout` wins; otherwise
        // `disable_timeout` or headless mode remove the limit; interactive mode
        // defaults to a 10-minute cap.
        let timeout = if explicit_timeout.is_some() {
            explicit_timeout
        } else if disable_timeout || context.is_headless {
            None
        } else {
            Some(BACKGROUND_DEFAULT_TIMEOUT_SECS)
        };

        // Persist the raw output to the session terminals dir when wired; otherwise
        // fall back to a temp file (stateless contexts such as tests/examples).
        // The filename embeds the unique `task_id` so concurrent runs on different
        // threads can't collide on the same temp path (kalid alone is only unique
        // per-thread).
        let task_id = next_background_task_id();
        let output_path = match &context.terminals_dir {
            Some(dir) => {
                let _ = std::fs::create_dir_all(dir);
                dir.join(format!("shell-{task_id}.txt")).to_string_lossy().to_string()
            }
            None => match env
                .create_temp_file(Some(CreateTempFileOptions {
                    prefix: format!("shell-{task_id}-"),
                    suffix: ".log".to_string(),
                    abort_token: signal.clone(),
                }))
                .await
            {
                HarnessResult::Ok(path) => path,
                HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
            },
        };

        spawn_background_shell(
            env.clone(),
            command.to_string(),
            cwd,
            timeout,
            task_id.clone(),
            output_path.clone(),
        );

        let timeout_label = timeout
            .map(|seconds| format!("{seconds}s"))
            .unwrap_or_else(|| "no limit".to_string());
        let text = format!(
            "Background task started: {description}\nTask ID: {task_id}\nOutput file: {output_path}\nTimeout: {timeout_label}"
        );
        return Ok(AgentToolResult {
            content: vec![ToolResultContent::Text(TextContent::new(text))],
            details: json!({
                "background": true,
                "taskId": task_id,
                "description": description,
                "outputPath": output_path,
                "timeout": timeout,
            }),
            added_tool_names: None,
            terminate: None,
            usage: None,
        });
    }

    let timeout = if disable_timeout { None } else { explicit_timeout };

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

    // Persist the full command output to the session terminals dir (when wired),
    // before `capture.output` is moved into the model-facing `text` below so it
    // survives session resume and is referenced from tool_outputs.jsonl.
    let persisted_output_path = persist_foreground_output(&context.terminals_dir, &id, &capture).await;

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

    // Cap the result to keep session message memory bounded. Long outputs are
    // still available on disk via `fullOutputPath` / `outputPath` in details.
    if text.chars().count() > MAX_TOOL_RESULT_CHARS {
        let mut cut: String = text.chars().take(MAX_TOOL_RESULT_CHARS).collect();
        if let Some(idx) = cut.rfind('\n')
            && idx > MAX_TOOL_RESULT_CHARS / 2
        {
            cut.truncate(idx);
        }
        let omitted = text.chars().count().saturating_sub(cut.chars().count());
        cut.push_str(&format!(
            "\n\n... [truncated {omitted} characters; full output available at {}]",
            capture
                .full_output_path
                .as_deref()
                .unwrap_or("the session terminals log")
        ));
        text = cut;
    }

    Ok(AgentToolResult {
        content: vec![ToolResultContent::Text(TextContent::new(text))],
        details: json!({
            "exitCode": capture.exit_code,
            "truncated": capture.truncated,
            "cancelled": capture.cancelled,
            "fullOutputPath": capture.full_output_path,
            "outputPath": persisted_output_path,
        }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
}

/// Next monotonic background task id (`bg-<n>`).
fn next_background_task_id() -> String {
    static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);
    format!("bg-{}", NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst))
}

/// Live background shell tasks indexed by task id, so a task can be cancelled
/// explicitly (`cancel_background_task`) or by a session teardown.
static BACKGROUND_TASKS: std::sync::OnceLock<StdMutex<std::collections::HashMap<String, CancellationToken>>> =
    std::sync::OnceLock::new();

fn background_tasks() -> &'static StdMutex<std::collections::HashMap<String, CancellationToken>> {
    BACKGROUND_TASKS.get_or_init(|| StdMutex::new(std::collections::HashMap::new()))
}

/// Register a background task token; returns the token bound to `task_id`.
fn register_background_task(task_id: String, token: CancellationToken) {
    // INVARIANT: the worker panic-safe because the task only panics if another
    // thread panicked while holding this mutex — which would crash the process
    // anyway (no cross-thread poison recovery makes sense here).
    background_tasks().lock().expect("bg tasks lock").insert(task_id, token);
}

/// Cancel a running background shell task by `task_id` (e.g. `bg-12`).
///
/// Terminates the process group (graceful SIGTERM → SIGKILL) via the abort
/// token the task's shell execution is listening on. Returns `true` when the
/// task existed and was cancelled.
pub fn cancel_background_task(task_id: &str) -> bool {
    // INVARIANT: see `register_background_task` — poison is unrecoverable.
    let token = background_tasks().lock().expect("bg tasks lock").remove(task_id);
    match token {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
}

/// List ids of currently registered background tasks.
pub fn list_background_tasks() -> Vec<String> {
    // INVARIANT: see `register_background_task` — poison is unrecoverable.
    background_tasks()
        .lock()
        .expect("bg tasks lock")
        .keys()
        .cloned()
        .collect()
}

/// Test-only: clear the background-task registry. Used to isolate tests that
/// spawn fire-and-forget background tasks, since the registry is process-global
/// and otherwise leaks across tests (spawned tasks may still be running when
/// the next test starts).
#[cfg(test)]
pub(crate) fn clear_background_tasks() {
    // INVARIANT: see `register_background_task` — poison is unrecoverable.
    background_tasks().lock().expect("bg tasks lock").clear();
}

/// Remove a completed background task from the registry (best-effort cleanup).
fn unregister_background_task(task_id: &str) {
    // INVARIANT: see `register_background_task` — poison is unrecoverable.
    background_tasks().lock().expect("bg tasks lock").remove(task_id);
}

/// Drop guard that unregisters a background task when it completes — including
/// on panic. Without this, a task that panics before calling
/// `unregister_background_task` would leak its id in the registry forever.
struct BackgroundTaskGuard {
    task_id: String,
}

impl Drop for BackgroundTaskGuard {
    fn drop(&mut self) {
        unregister_background_task(&self.task_id);
    }
}

/// Write the full foreground `shell_exec` output to the session terminals dir.
///
/// Prefers copying the already-written full-output temp file (when the capture
/// was truncated) to avoid re-copying truncated text; otherwise writes the
/// captured output directly. Returns `None` when no terminals dir is configured
/// (stateless context). Failures are logged and swallowed — output persistence
/// is best-effort and never fails the tool call.
async fn persist_foreground_output(
    terminals_dir: &Option<std::path::PathBuf>,
    call_id: &str,
    capture: &ShellCaptureResult,
) -> Option<String> {
    let dir = terminals_dir.as_ref()?;
    let _ = tokio::fs::create_dir_all(dir).await;
    let path = dir.join(format!("shell-{call_id}.txt"));
    let path_str = path.to_string_lossy().to_string();
    let wrote = if let Some(full) = &capture.full_output_path {
        std::fs::copy(full, &path).is_ok()
    } else {
        tokio::fs::write(&path, &capture.output).await.is_ok()
    };
    wrote.then_some(path_str)
}

/// Spawn a shell command as a detached background task.
///
/// The process runs independently of the agent turn: it owns a fresh abort
/// token (not the turn's cancellation token) and streams stdout/stderr to the
/// given output file. The task is fire-and-forget; its completion status is
/// appended to the file as a footer.
///
/// The task is registered under `task_id` via [`register_background_task`] so it
/// can be cancelled explicitly with [`cancel_background_task`] — cancellation
/// terminates the whole process group (SIGTERM → SIGKILL), not just the shell.
fn spawn_background_shell(
    env: Arc<LocalExecutionEnv>,
    command: String,
    cwd: String,
    timeout: Option<u64>,
    task_id: String,
    output_path: String,
) {
    let file = match std::fs::OpenOptions::new().create(true).append(true).open(&output_path) {
        Ok(file) => Arc::new(StdMutex::new(file)),
        Err(error) => {
            log::warn!("shell_exec background: cannot open output file {output_path}: {error}");
            return;
        }
    };

    let make_writer = |file: Arc<StdMutex<std::fs::File>>| -> ShellOutputCallback {
        Arc::new(move |chunk: &str| {
            if let Ok(mut handle) = file.lock() {
                let _ = handle.write_all(chunk.as_bytes());
                let _ = handle.write_all(b"\n");
                let _ = handle.flush();
            }
        })
    };
    let on_stdout = make_writer(file.clone());
    let on_stderr = make_writer(file.clone());

    let cancel_token = CancellationToken::new();
    let registry_task_id = task_id.clone();

    let footer_file = file.clone();
    tokio::spawn(async move {
        register_background_task(registry_task_id.clone(), cancel_token.clone());
        // Ensure the task is unregistered even if we panic mid-flight (e.g. a
        // callback or lock poisoning). The guard runs before footer write, so
        // a panic during exec still cleans up the registry.
        let _guard = BackgroundTaskGuard {
            task_id: registry_task_id.clone(),
        };
        let result = env
            .exec(
                &command,
                Some(ShellExecOptions {
                    cwd: Some(cwd),
                    timeout,
                    abort_token: Some(cancel_token),
                    on_stdout: Some(on_stdout),
                    on_stderr: Some(on_stderr),
                    ..Default::default()
                }),
            )
            .await;

        let footer = match &result {
            HarnessResult::Ok(output) => format!("\n\n[exit code: {}]", output.exit_code),
            HarnessResult::Err(error) => format!("\n\n[shell_exec error: {}]", error.message),
        };
        if let Ok(mut handle) = footer_file.lock() {
            let _ = handle.write_all(footer.as_bytes());
            let _ = handle.flush();
        }
        // `_guard` drops here and unregisters the task.
    });
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

        let ctx = crate::tools::types::ToolContext::new(env.clone());
        let result = execute_shell_exec(
            env,
            "t-stream".to_string(),
            json!({ "command": "printf early; sleep 0.2; printf late", "timeout": 5 }),
            None,
            Some(on_update),
            ctx,
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

    #[tokio::test]
    async fn shell_exec_background_requires_description() {
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let ctx = crate::tools::types::ToolContext::new(env.clone());
        let result = execute_shell_exec(
            env,
            "t-req".to_string(),
            json!({ "command": "echo hi", "run_in_background": true }),
            None,
            None,
            ctx,
        )
        .await;
        assert!(result.is_err(), "expected error when description is missing");
        assert!(result.unwrap_err().to_string().contains("description is required"));
    }

    #[tokio::test]
    async fn shell_exec_background_spawns_and_writes_output_file() {
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let ctx = crate::tools::types::ToolContext::new(env.clone());
        let result = execute_shell_exec(
            env,
            "t-spawn".to_string(),
            json!({ "command": "echo hello-from-bg", "run_in_background": true, "description": "demo bg task" }),
            None,
            None,
            ctx,
        )
        .await
        .expect("shell_exec execution");

        assert_eq!(result.details.get("background").and_then(Value::as_bool), Some(true));
        let task_id = result.details.get("taskId").and_then(Value::as_str).unwrap_or_default();
        assert!(task_id.starts_with("bg-"), "{task_id}");
        let output_path = result
            .details
            .get("outputPath")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(!output_path.is_empty(), "expected output path");

        // The tool returns immediately; wait for the detached process to finish writing.
        let mut waited = 0;
        let mut contents = String::new();
        while waited < 50 {
            if let Ok(text) = std::fs::read_to_string(output_path)
                && text.contains("hello-from-bg")
                && text.contains("[exit code: 0]")
            {
                contents = text;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            waited += 1;
        }
        assert!(contents.contains("hello-from-bg"), "output file: {contents}");
        assert!(contents.contains("[exit code: 0]"), "output file: {contents}");
    }

    #[tokio::test]
    async fn shell_exec_background_default_timeout_is_600_when_interactive() {
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let ctx = crate::tools::types::ToolContext::new(env.clone());
        let result = execute_shell_exec(
            env,
            "t-600".to_string(),
            json!({ "command": "true", "run_in_background": true, "description": "d" }),
            None,
            None,
            ctx,
        )
        .await
        .expect("shell_exec execution");
        assert_eq!(result.details.get("timeout").and_then(Value::as_u64), Some(600));
    }

    #[tokio::test]
    async fn shell_exec_background_no_timeout_when_disable_timeout() {
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let ctx = crate::tools::types::ToolContext::new(env.clone());
        let result = execute_shell_exec(
            env,
            "t-disable".to_string(),
            json!({ "command": "true", "run_in_background": true, "disable_timeout": true, "description": "d" }),
            None,
            None,
            ctx,
        )
        .await
        .expect("shell_exec execution");
        assert!(result.details["timeout"].is_null(), "expected null timeout");
    }

    #[tokio::test]
    async fn shell_exec_background_no_timeout_when_headless() {
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let ctx = crate::tools::types::ToolContext::new(env.clone()).with_headless(true);
        let result = execute_shell_exec(
            env,
            "t-headless".to_string(),
            json!({ "command": "true", "run_in_background": true, "description": "d" }),
            None,
            None,
            ctx,
        )
        .await
        .expect("shell_exec execution");
        assert!(result.details["timeout"].is_null(), "expected null timeout in headless mode");
    }

    #[tokio::test]
    async fn shell_exec_foreground_persists_output_to_terminals_dir() {
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let terminals = TempDir::new().expect("terminals dir");
        let ctx =
            crate::tools::types::ToolContext::new(env.clone()).with_terminals_dir(Some(terminals.path().to_path_buf()));
        let result = execute_shell_exec(
            env,
            "t-fg".to_string(),
            json!({ "command": "echo persisted-output" }),
            None,
            None,
            ctx,
        )
        .await
        .expect("shell_exec execution");

        let output_path = result
            .details
            .get("outputPath")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(!output_path.is_empty(), "expected outputPath in details");
        assert!(output_path.ends_with("shell-t-fg.txt"), "{output_path}");
        let contents = std::fs::read_to_string(output_path).expect("read terminals file");
        assert!(contents.contains("persisted-output"), "terminals file: {contents}");
    }

    #[tokio::test]
    async fn background_task_can_be_cancelled() {
        // The background-task registry is process-global; clear it so a task
        // leaked by a previous test (still running its footer write) can't
        // pollute this test's assertions.
        clear_background_tasks();
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let ctx = crate::tools::types::ToolContext::new(env.clone());
        let result = execute_shell_exec(
            env,
            "t-cancel".to_string(),
            json!({ "command": "sleep 60", "run_in_background": true, "description": "cancel me" }),
            None,
            None,
            ctx,
        )
        .await
        .expect("shell_exec execution");

        let task_id = result
            .details
            .get("taskId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        assert!(task_id.starts_with("bg-"), "{task_id}");
        let output_path = result
            .details
            .get("outputPath")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // Ensure the task is registered (spawn is async — small grace).
        let mut registered = false;
        for _ in 0..50 {
            if crate::tools::shell_exec::list_background_tasks().contains(&task_id) {
                registered = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(registered, "task {task_id} was never registered");

        // Cancelling must remove it and stop the process group.
        assert!(
            crate::tools::shell_exec::cancel_background_task(&task_id),
            "cancel returned false"
        );
        assert!(
            !crate::tools::shell_exec::cancel_background_task(&task_id),
            "double cancel should be false"
        );

        // The footer should reflect an aborted run (process killed), and because it
        // was terminated not cleanly exited, we expect no "[exit code: 0]" footer.
        let mut waited = 0;
        let mut contents = String::new();
        while waited < 50 {
            if let Ok(text) = std::fs::read_to_string(&output_path) {
                contents = text;
                if !contents.is_empty() {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            waited += 1;
        }
        // Either the footer shows a non-zero/error, or nothing (still killed in flight).
        assert!(
            contents.is_empty() || !contents.contains("[exit code: 0]"),
            "cancelled task must not report exit code 0: {contents}"
        );
    }

    #[tokio::test]
    async fn shell_exec_background_persists_output_to_terminals_dir() {
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let terminals = TempDir::new().expect("terminals dir");
        let ctx =
            crate::tools::types::ToolContext::new(env.clone()).with_terminals_dir(Some(terminals.path().to_path_buf()));
        let result = execute_shell_exec(
            env,
            "t-bgf".to_string(),
            json!({ "command": "echo bg-persisted", "run_in_background": true, "description": "d" }),
            None,
            None,
            ctx,
        )
        .await
        .expect("shell_exec execution");

        let output_path = result
            .details
            .get("outputPath")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(output_path.ends_with(".txt"), "{output_path}");
        // The tool returns immediately; wait for the detached process to finish writing.
        let mut waited = 0;
        let mut contents = String::new();
        while waited < 50 {
            if let Ok(text) = std::fs::read_to_string(output_path)
                && text.contains("bg-persisted")
                && text.contains("[exit code: 0]")
            {
                contents = text;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            waited += 1;
        }
        assert!(contents.contains("bg-persisted"), "terminals file: {contents}");
    }

    #[test]
    fn strip_redundant_cd_prefix_removes_matching_prefix() {
        let cwd = "/Users/me/project";
        assert_eq!(
            strip_redundant_cd_prefix("cd /Users/me/project && cargo test", cwd),
            "cargo test"
        );
        assert_eq!(
            strip_redundant_cd_prefix("cd /Users/me/project/ && cargo test", cwd),
            "cargo test"
        );
        assert_eq!(strip_redundant_cd_prefix("cd /Users/me/project; cargo test", cwd), "cargo test");
        assert_eq!(strip_redundant_cd_prefix("cd /Users/me/project;cargo test", cwd), "cargo test");
        assert_eq!(strip_redundant_cd_prefix("cd /Users/me/project&&make", cwd), "make");
        assert_eq!(strip_redundant_cd_prefix("cd /Users/me/project\ncargo test", cwd), "cargo test");
        assert_eq!(strip_redundant_cd_prefix("cd . && make", cwd), "make");
        assert_eq!(strip_redundant_cd_prefix("cd src/.. && make", cwd), "make");
    }

    #[test]
    fn strip_redundant_cd_prefix_keeps_nonmatching_commands() {
        let cwd = "/Users/me/project";
        assert_eq!(
            strip_redundant_cd_prefix("cd /Users/me/other && cargo test", cwd),
            "cd /Users/me/other && cargo test"
        );
        assert_eq!(
            strip_redundant_cd_prefix("cd /Users/me/project2 && cargo test", cwd),
            "cd /Users/me/project2 && cargo test"
        );
        assert_eq!(strip_redundant_cd_prefix("cargo test", cwd), "cargo test");
        assert_eq!(strip_redundant_cd_prefix("cd ~ && cargo test", cwd), "cd ~ && cargo test");
        // `cd` as the entire command is kept rather than collapsing to an empty command.
        assert_eq!(strip_redundant_cd_prefix("cd /Users/me/project", cwd), "cd /Users/me/project");
    }

    #[test]
    fn normalize_shell_exec_args_strips_redundant_cd_prefix() {
        let cwd = "/Users/me/project";
        assert_eq!(
            normalize_shell_exec_args(r#"{"command":"cd /Users/me/project && make test"}"#, cwd),
            r#"{"command":"make test"}"#
        );
        assert_eq!(
            normalize_shell_exec_args(r#"{"command":"cd /Users/me/project && make","timeout":60}"#, cwd),
            r#"{"command":"make","timeout":60}"#
        );
    }

    #[test]
    fn normalize_shell_exec_args_passes_through_unrelated_input() {
        let cwd = "/Users/me/project";
        let raw = r#"{"command":"make test","timeout":60}"#;
        assert_eq!(normalize_shell_exec_args(raw, cwd), raw);
        assert_eq!(normalize_shell_exec_args("not json", cwd), "not json");
        assert_eq!(normalize_shell_exec_args(r#"{"path":"src"}"#, cwd), r#"{"path":"src"}"#);
    }

    #[test]
    fn shell_exec_description_embeds_working_directory() {
        let temp = TempDir::new().expect("temp dir");
        let env = Arc::new(LocalExecutionEnv::new(temp.path().to_path_buf()));
        let cwd = env.cwd().to_string();
        let tool = create_shell_exec_tool(env);
        let description = tool.tool.description.clone();
        assert!(description.contains(&cwd), "{description}");
        assert!(description.contains("do NOT prefix"), "{description}");
    }
}
