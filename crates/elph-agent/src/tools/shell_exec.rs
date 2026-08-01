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
                 or any other `cd ... &&`. Output truncated to last {DEFAULT_MAX_LINES} lines or {}/KB.",
                DEFAULT_MAX_BYTES / 1024
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute (runs in the working directory)" },
                    "timeout": { "type": "number", "description": "Timeout in seconds" }
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
        move |_id,
              args,
              signal,
              on_update,
              context|
              -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
            let env = context.env.clone();
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
        usage: None,
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
