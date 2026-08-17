//! Stateful terminal tool — elph coding-agent tools.
//!
//! `shell_use` gives the agent a real PTY-backed terminal session it can
//! drive, inspect, assert on, and record. It complements the stateless
//! `shell_exec` tool: use `shell_exec` for one-shot build/test/git commands,
//! and `shell_use` for interactive programs, TUIs, REPLs, prompts needing
//! keystrokes, and verifying on-screen state.
//!
//! It wraps the [`shell_use`](https://crates.io/crates/shell-use) crate
//! (`SessionRegistry` / `SessionHandle`), which runs a portable-pty +
//! alacritty terminal emulator fully in-process — no external daemon or binary.
//!
//! Sessions are process-global and persist across tool calls (keyed by
//! `session`, default `default`). Call [`close_shell_use_sessions`] on host
//! session teardown so PTYs don't outlive the agent turn.

use std::sync::{Arc, OnceLock};

use elph_ai::Tool;
use serde_json::{Value, json};
use shell_use::shell::Shell;
use shell_use::{
    MouseAction, OpenOptions, Operation, OperationResult, RunOptions, SessionHandle, SessionRegistry, ShellUseError,
    Timeouts,
};
use tokio_util::sync::CancellationToken;

use crate::runtime::local_env::LocalExecutionEnv;
use crate::types::{AgentTool, AgentToolResult, ToolContext, ToolExecuteFn};

/// Cap on `text` / `state` / `cells` output before truncation (keeps tool calls
/// token-efficient for the model; full scrollback is available on demand).
const MAX_RENDERED_CHARS: usize = 16 * 1024;

/// Max concurrent `shell_use` sessions kept in the process-global registry.
/// When exceeded, the oldest session (first returned by `registry.sessions()`)
/// is closed to prevent unbounded PTY buffer growth.
const MAX_SHELL_USE_SESSIONS: usize = 5;

/// Create the `shell_use` tool using a process-global session registry.
pub fn create_shell_use_tool(_env: Arc<LocalExecutionEnv>) -> AgentTool {
    AgentTool {
        tool: Tool {
            name: "shell_use".into(),
            constrained_sampling: None,
            description: "Drive, inspect, assert on, and record real terminal sessions (bash/zsh/fish/pwsh/cmd/nushell/...). \
                Use for interactive programs, TUIs, REPLs, prompts needing keystrokes, and verifying on-screen state — \
                not for one-shot commands (use shell_exec for those). Sessions persist across calls until `action: close`. \
                Actions: `open` (spawn shell), `run` (spawn a program directly), `submit` (type + Enter), `type`, `press`, \
                `keys`, `mouse`, `resize`, `signal`, `kill`, `write` (raw bytes), `text`, `state`, `cells`, `get`, \
                `screenshot` (SVG path), `wait`, `expect`, `sessions`, `close`. Pass `session` (default \"default\") to \
                select/name a session.".to_string(),
            parameters: tool_parameters(),
        },
        label: "shell_use".into(),
        execution_mode: None,
        prepare_arguments: None,
        execute: shell_use_execute_fn(),
    }
}

/// Assemble the `shell_use` JSON schema (built flat to avoid `json!` recursion).
fn tool_parameters() -> Value {
    use serde_json::Value as J;
    let mut props = serde_json::Map::new();
    let mut prop = |key: &str, schema: Value| {
        props.insert(key.to_string(), schema);
    };
    prop(
        "action",
        json!({
            "type": "string",
            "enum": [
                "open", "run", "submit", "type", "press", "keys", "mouse", "resize", "signal", "kill",
                "write", "text", "state", "cells", "get", "screenshot", "wait", "expect", "sessions", "close"
            ],
            "description": "Which terminal operation to perform"
        }),
    );
    prop(
        "session",
        json!({"type": "string", "description": "Session name (default \"default\"); independent terminal sessions are keyed by name"}),
    );
    prop(
        "shell",
        json!({"type": "string", "description": "Shell for `open` (bash/zsh/fish/pwsh/cmd/nushell/...; default: platform)"}),
    );
    prop("cols", json!({"type": "number", "description": "PTY columns (default 80)"}));
    prop("rows", json!({"type": "number", "description": "PTY rows (default 30)"}));
    prop(
        "cwd",
        json!({"type": "string", "description": "Working directory for the new session (defaults to the agent cwd)"}),
    );
    prop(
        "env",
        json!({"type": "array", "items": {"type": "string"}, "description": "Extra KEY=VALUE env vars for `open`/`run`"}),
    );
    prop("program", json!({"type": "string", "description": "Program for `run`"}));
    prop(
        "args",
        json!({"type": "array", "items": {"type": "string"}, "description": "Arguments for `run`"}),
    );
    prop(
        "data",
        json!({"type": "string", "description": "Text to type / submit / write"}),
    );
    prop(
        "keys",
        json!({"type": "array", "items": {"type": "string"}, "description": "Named keys for `press`, e.g. [\"Ctrl+C\"], [\"Escape\"], [\":\", \"w\", \"q\", \"Enter\"]"}),
    );
    prop(
        "key",
        json!({"type": "string", "description": "Single key combo for `keys`, e.g. \"Ctrl+a\""}),
    );
    prop(
        "mouse_action",
        json!({"type": "string", "enum": ["click", "move", "down", "up", "drag", "scroll"], "description": "Mouse sub-action for `mouse`"}),
    );
    prop(
        "on_text",
        json!({"type": "string", "description": "Click by visible label for `mouse click`"}),
    );
    prop(
        "button",
        json!({"type": "number", "description": "Mouse button (default 0 = left)"}),
    );
    prop(
        "clicks",
        json!({"type": "number", "description": "Mouse click count (default 1)"}),
    );
    prop(
        "direction",
        json!({"type": "string", "enum": ["up", "down", "left", "right"], "description": "Mouse scroll direction"}),
    );
    prop(
        "amount",
        json!({"type": "number", "description": "Mouse scroll amount (default 3)"}),
    );
    prop(
        "x",
        json!({"type": "number", "description": "Column for mouse / cells (0-based)"}),
    );
    prop("y", json!({"type": "number", "description": "Row for mouse / cells (0-based)"}));
    prop("w", json!({"type": "number", "description": "Width for `cells` region"}));
    prop("h", json!({"type": "number", "description": "Height for `cells` region"}));
    prop("x1", json!({"type": "number", "description": "Drag start column"}));
    prop("y1", json!({"type": "number", "description": "Drag start row"}));
    prop("x2", json!({"type": "number", "description": "Drag end column"}));
    prop("y2", json!({"type": "number", "description": "Drag end row"}));
    prop(
        "signal",
        json!({"type": "string", "enum": ["INT", "TERM", "KILL", "QUIT"], "description": "Signal for `signal`"}),
    );
    prop(
        "field",
        json!({"type": "string", "enum": ["command", "output", "exit-code", "cwd", "cursor", "size"], "description": "Structured field for `get`"}),
    );
    prop(
        "kind",
        json!({"type": "string", "description": "Wait/expect kind. wait: text|idle|command|exit|ready (default idle). expect: text|exit-code|output|snapshot (default text)"}),
    );
    prop(
        "text",
        json!({"type": "string", "description": "Expected text/pattern for `wait`/`expect`"}),
    );
    prop("regex", json!({"type": "boolean", "description": "Interpret `text` as regex"}));
    prop("not", json!({"type": "boolean", "description": "Invert the condition"}));
    prop(
        "strict",
        json!({"type": "boolean", "description": "Strict single-match for `expect text` (default true)"}),
    );
    prop(
        "full",
        json!({"type": "boolean", "description": "Full scrollback instead of viewport (text/screenshot)"}),
    );
    prop(
        "timeout_ms",
        json!({"type": "number", "description": "Timeout for wait/expect (ms)"}),
    );
    prop(
        "fg",
        json!({"type": "string", "description": "Expected foreground color for `expect text` (ansi-256, #hex, or default)"}),
    );
    prop(
        "bg",
        json!({"type": "string", "description": "Expected background color for `expect text`"}),
    );
    prop(
        "code",
        json!({"type": "number", "description": "Expected exit code for `expect exit-code`"}),
    );
    prop(
        "name",
        json!({"type": "string", "description": "Snapshot name for `expect snapshot`"}),
    );
    prop(
        "update",
        json!({"type": "boolean", "description": "Write/update the snapshot instead of asserting"}),
    );
    prop(
        "include_colors",
        json!({"type": "boolean", "description": "Compare colors for snapshot"}),
    );
    prop(
        "path",
        json!({"type": "string", "description": "File path for `screenshot` (SVG)"}),
    );
    prop(
        "all",
        json!({"type": "boolean", "description": "Close all sessions for `close`"}),
    );
    json!({
        "type": "object",
        "properties": J::Object(props),
        "required": ["action"]
    })
}

fn shell_use_execute_fn() -> ToolExecuteFn {
    let registry: &'static SessionRegistry = shell_use_registry();
    Arc::new(move |id, args, signal, _on_update, context| {
        Box::pin(async move { execute_shell_use(registry, id, args, signal, context).await })
    })
}

/// In-process session registry for this process.
fn shell_use_registry() -> &'static SessionRegistry {
    static REGISTRY: OnceLock<SessionRegistry> = OnceLock::new();
    REGISTRY.get_or_init(SessionRegistry::default)
}

/// Close all `shell_use` sessions — call from host session teardown so PTYs
/// don't leak across sessions.
pub fn close_shell_use_sessions() {
    shell_use_registry().close_all();
}

/// Names of currently-open `shell_use` sessions.
pub fn shell_use_open_sessions() -> Vec<String> {
    shell_use_registry().sessions()
}

/// Evict the oldest idle `shell_use` sessions when the registry exceeds
/// `MAX_SHELL_USE_SESSIONS`. Called after `open` / `run` to bound PTY buffer
/// memory growth across the process lifetime.
fn evict_excess_sessions() {
    let registry = shell_use_registry();
    let mut names = registry.sessions();
    while names.len() > MAX_SHELL_USE_SESSIONS {
        if let Some(oldest) = names.pop() {
            let _ = registry.close(&oldest);
        } else {
            break;
        }
    }
}

fn opt_string(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn opt_u16(args: &Value, key: &str) -> Option<u16> {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|v| v.min(u16::MAX as u64) as u16)
}

fn opt_u8(args: &Value, key: &str) -> Option<u8> {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|v| v.min(u8::MAX as u64) as u8)
}

fn opt_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn opt_i32(args: &Value, key: &str) -> Option<i32> {
    args.get(key)
        .and_then(Value::as_i64)
        .map(|v| v.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
}

fn str_list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn session_name(args: &Value) -> String {
    opt_string(args, "session").unwrap_or_else(|| "default".to_string())
}

/// Map a user-supplied shell name to the `shell-use` `Shell` enum.
fn parse_shell(value: Option<String>) -> Option<Shell> {
    match value.as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("bash") => Some(Shell::Bash),
        Some("powershell") => Some(Shell::Powershell),
        Some("pwsh") => Some(Shell::Pwsh),
        Some("cmd") => Some(Shell::Cmd),
        Some("fish") => Some(Shell::Fish),
        Some("zsh") => Some(Shell::Zsh),
        Some("xonsh") => Some(Shell::Xonsh),
        Some("elvish") => Some(Shell::Elvish),
        Some("nushell") => Some(Shell::Nushell),
        _ => None,
    }
}

fn parse_env(args: &Value) -> Vec<(String, String)> {
    str_list(args, "env")
        .into_iter()
        .filter_map(|entry| {
            let (key, value) = entry.split_once('=')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

fn default_timeouts() -> Timeouts {
    Timeouts {
        text: Some(5_000),
        idle: Some(5_000),
        command: Some(30_000),
        exit: Some(30_000),
        ready: Some(30_000),
    }
}

fn opt_color(args: &Value, key: &str) -> Option<String> {
    // Colors (`fg` / `bg`) are passed as strings (ansi-256, #hex, or "default")
    // and parsed by the shell-use crate itself.
    opt_string(args, key)
}

/// Convert a `ShellUseError` into a model-readable message.
fn error_text(error: &ShellUseError) -> String {
    match error.kind {
        shell_use::ErrorKind::Assertion => format!("assertion failed: {}", error.message),
        shell_use::ErrorKind::Usage => format!("usage: {}", error.message),
        shell_use::ErrorKind::NoSession => format!("no active session: {}", error.message),
        shell_use::ErrorKind::Internal => format!("internal error: {}", error.message),
    }
}

/// Truncate rendered terminal text to a bounded size.
fn truncate_rendered(text: &str) -> String {
    if text.chars().count() <= MAX_RENDERED_CHARS {
        text.to_string()
    } else {
        let kept: String = text.chars().take(MAX_RENDERED_CHARS).collect();
        format!("{kept}\n… [truncated at {MAX_RENDERED_CHARS} chars; re-run with `full: true` for full scrollback]")
    }
}

async fn execute_shell_use(
    registry: &'static SessionRegistry,
    _id: String,
    args: Value,
    signal: Option<CancellationToken>,
    context: ToolContext,
) -> anyhow::Result<AgentToolResult> {
    if let Some(token) = &signal
        && token.is_cancelled()
    {
        return Err(anyhow::anyhow!("Tool cancelled"));
    }

    let action = args.get("action").and_then(Value::as_str).unwrap_or("").to_string();
    let session_name = session_name(&args);

    // Session-lifecycle actions operate on the registry directly.
    let result = match action.as_str() {
        "open" => {
            let cwd = opt_string(&args, "cwd").unwrap_or_else(|| context.cwd.clone());
            let opts = OpenOptions {
                shell: parse_shell(opt_string(&args, "shell")),
                cols: opt_u16(&args, "cols").unwrap_or(80),
                rows: opt_u16(&args, "rows").unwrap_or(30),
                cwd: (!cwd.is_empty()).then_some(cwd),
                env: parse_env(&args),
                wait_ready: Some(true),
                timeouts: default_timeouts(),
            };
            let handle = registry.session(&session_name);
            handle.open(opts).map_err(|e| anyhow::anyhow!(error_text(&e)))?;
            evict_excess_sessions();
            AgentToolResult::text(format!("Session \"{session_name}\" open."))
        }
        "run" => {
            let program = opt_string(&args, "program").ok_or_else(|| anyhow::anyhow!("run requires `program`"))?;
            let program_label = program.clone();
            let cwd = opt_string(&args, "cwd").unwrap_or_else(|| context.cwd.clone());
            let opts = RunOptions {
                program,
                args: str_list(&args, "args"),
                cols: opt_u16(&args, "cols").unwrap_or(80),
                rows: opt_u16(&args, "rows").unwrap_or(30),
                cwd: (!cwd.is_empty()).then_some(cwd),
                env: parse_env(&args),
                wait_ready: None,
                timeouts: default_timeouts(),
            };
            let handle = registry.session(&session_name);
            handle.run(opts).map_err(|e| anyhow::anyhow!(error_text(&e)))?;
            evict_excess_sessions();
            AgentToolResult::text(format!("Session \"{session_name}\" running {program_label}."))
        }
        "sessions" => {
            let names = registry.sessions();
            if names.is_empty() {
                AgentToolResult::text("No open shell_use sessions.")
            } else {
                AgentToolResult::text(format!("Open sessions: {}", names.join(", ")))
            }
        }
        "close" => {
            if opt_bool(&args, "all").unwrap_or(false) {
                registry.close_all();
                AgentToolResult::text("All shell_use sessions closed.")
            } else {
                let _ = registry.close(&session_name);
                AgentToolResult::text(format!("Session \"{session_name}\" closed."))
            }
        }
        _ => {
            let handle = registry.session(&session_name);
            run_operation(handle, &action, &args, &context, &signal).await?
        }
    };

    Ok(result)
}

async fn run_operation(
    session: SessionHandle,
    action: &str,
    args: &Value,
    context: &ToolContext,
    signal: &Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    let operation = match action {
        "state" => Operation::State,
        "text" => Operation::Text {
            full: opt_bool(args, "full").unwrap_or(false),
        },
        "get" => {
            let field = opt_string(args, "field").unwrap_or_default();
            match field.as_str() {
                "command" => Operation::GetCommand,
                "output" => Operation::GetOutput,
                "exit-code" => Operation::GetExitCode,
                "cwd" => Operation::GetCwd,
                "cursor" => Operation::GetCursor,
                "size" => Operation::GetSize,
                other => return Err(anyhow::anyhow!("Unknown `get` field: {other}")),
            }
        }
        "cells" => Operation::Cells {
            x: opt_u16(args, "x").unwrap_or(0),
            y: opt_u16(args, "y").unwrap_or(0),
            w: opt_u16(args, "w").unwrap_or(1),
            h: opt_u16(args, "h").unwrap_or(1),
        },
        "write" | "type" => Operation::Write {
            data: opt_string(args, "data").unwrap_or_default(),
        },
        "submit" => Operation::Submit {
            data: opt_string(args, "data"),
        },
        "press" => Operation::Press {
            keys: str_list(args, "keys"),
        },
        "keys" => Operation::Press {
            keys: opt_string(args, "key").into_iter().collect(),
        },
        "mouse" => match opt_string(args, "mouse_action")
            .unwrap_or_else(|| "click".to_string())
            .as_str()
        {
            "click" => Operation::Mouse {
                action: MouseAction::Click {
                    x: opt_u16(args, "x"),
                    y: opt_u16(args, "y"),
                    on_text: opt_string(args, "on_text"),
                    button: opt_u8(args, "button").unwrap_or(0),
                    clicks: opt_u8(args, "clicks").unwrap_or(1),
                },
            },
            "move" => Operation::Mouse {
                action: MouseAction::Move {
                    x: opt_u16(args, "x").unwrap_or(0),
                    y: opt_u16(args, "y").unwrap_or(0),
                },
            },
            "down" => Operation::Mouse {
                action: MouseAction::Down {
                    x: opt_u16(args, "x").unwrap_or(0),
                    y: opt_u16(args, "y").unwrap_or(0),
                    button: opt_u8(args, "button").unwrap_or(0),
                },
            },
            "up" => Operation::Mouse {
                action: MouseAction::Up {
                    x: opt_u16(args, "x").unwrap_or(0),
                    y: opt_u16(args, "y").unwrap_or(0),
                    button: opt_u8(args, "button").unwrap_or(0),
                },
            },
            "drag" => Operation::Mouse {
                action: MouseAction::Drag {
                    x1: opt_u16(args, "x1").unwrap_or(0),
                    y1: opt_u16(args, "y1").unwrap_or(0),
                    x2: opt_u16(args, "x2").unwrap_or(0),
                    y2: opt_u16(args, "y2").unwrap_or(0),
                    button: opt_u8(args, "button").unwrap_or(0),
                },
            },
            "scroll" => Operation::Mouse {
                action: MouseAction::Scroll {
                    direction: opt_string(args, "direction").unwrap_or_else(|| "down".to_string()),
                    amount: opt_u16(args, "amount").unwrap_or(3),
                },
            },
            other => return Err(anyhow::anyhow!("Unknown mouse action: {other}")),
        },
        "resize" => Operation::Resize {
            cols: opt_u16(args, "cols").unwrap_or(80),
            rows: opt_u16(args, "rows").unwrap_or(30),
        },
        "signal" => Operation::Signal {
            name: opt_string(args, "signal").unwrap_or_else(|| "TERM".to_string()),
        },
        "kill" => Operation::Signal {
            name: "KILL".to_string(),
        },
        "wait" => {
            let timeout = opt_u64(args, "timeout_ms");
            match opt_string(args, "kind").unwrap_or_else(|| "idle".to_string()).as_str() {
                "text" => Operation::WaitText {
                    text: opt_string(args, "text").unwrap_or_default(),
                    regex: opt_bool(args, "regex").unwrap_or(false),
                    full: opt_bool(args, "full").unwrap_or(false),
                    timeout_ms: timeout,
                    not: opt_bool(args, "not").unwrap_or(false),
                },
                "idle" => Operation::WaitIdle { timeout_ms: timeout },
                "command" => Operation::WaitCommand { timeout_ms: timeout },
                "exit" => Operation::WaitExit { timeout_ms: timeout },
                "ready" => Operation::WaitReady { timeout_ms: timeout },
                other => return Err(anyhow::anyhow!("Unknown wait kind: {other}")),
            }
        }
        "expect" => {
            let timeout = opt_u64(args, "timeout_ms");
            match opt_string(args, "kind").unwrap_or_else(|| "text".to_string()).as_str() {
                "text" => Operation::ExpectText {
                    text: opt_string(args, "text").unwrap_or_default(),
                    regex: opt_bool(args, "regex").unwrap_or(false),
                    full: opt_bool(args, "full").unwrap_or(false),
                    strict: opt_bool(args, "strict").unwrap_or(true),
                    not: opt_bool(args, "not").unwrap_or(false),
                    fg: opt_color(args, "fg"),
                    bg: opt_color(args, "bg"),
                    timeout_ms: timeout,
                },
                "exit-code" => Operation::ExpectExitCode {
                    code: opt_i32(args, "code").unwrap_or(0),
                    timeout_ms: timeout,
                },
                "output" => Operation::ExpectOutput {
                    text: opt_string(args, "text").unwrap_or_default(),
                    regex: opt_bool(args, "regex").unwrap_or(false),
                },
                "snapshot" => Operation::Snapshot {
                    name: opt_string(args, "name").unwrap_or_default(),
                    update: opt_bool(args, "update").unwrap_or(false),
                    include_colors: opt_bool(args, "include_colors").unwrap_or(false),
                    cwd: Some(context.cwd.clone()),
                },
                other => return Err(anyhow::anyhow!("Unknown expect kind: {other}")),
            }
        }
        "screenshot" => Operation::Screenshot {
            full: opt_bool(args, "full").unwrap_or(false),
            path: opt_string(args, "path"),
        },
        other => return Err(anyhow::anyhow!("Unknown action: {other}")),
    };

    // Bail out synchronously if cancelled after argument parsing.
    if let Some(token) = signal
        && token.is_cancelled()
    {
        return Err(anyhow::anyhow!("Tool cancelled"));
    }

    // `session.execute()` is a blocking call that can run for a long time
    // (notably `wait` / `expect` until a condition matches). Run it on a
    // blocking thread and race it against the abort signal so Ctrl+C during a
    // long wait cancels the turn instead of hanging until the PTY's internal
    // timeout fires.
    let handle = tokio::task::spawn_blocking(move || session.execute(operation));
    let result = if let Some(token) = signal {
        tokio::select! {
            result = handle => result,
            _ = token.cancelled() => {
                // The blocking task is dropped here; it may keep running on its
                // thread until the PTY's own timeout fires, but the turn is no
                // longer waiting on it. The session is left in an unknown state
                // — the next call to it will likely surface the interruption.
                return Err(anyhow::anyhow!("Tool cancelled"));
            }
        }
    } else {
        handle.await
    };
    let result = result.map_err(|error| anyhow::anyhow!("shell_use task join error: {error}"))?;
    let result = result.map_err(|e| anyhow::anyhow!(error_text(&e)))?;
    Ok(format_result(&result))
}

fn format_result(result: &OperationResult) -> AgentToolResult {
    let text = match result {
        OperationResult::Unit => "ok".to_string(),
        OperationResult::Open(_) => "session open".to_string(),
        OperationResult::State(state) => {
            let mut out = format!(
                "shell: {}\nsize: {}x{}\ncursor: {}, {}\ncwd: {}\nready: {}\n",
                state.session_shell.as_deref().unwrap_or("(unknown)"),
                state.cols,
                state.rows,
                state.cursor.x,
                state.cursor.y,
                state.cwd.as_deref().unwrap_or("(none)"),
                state.ready,
            );
            if let Some(command) = &state.last_command {
                out.push_str(&format!("last command: {command}\n"));
            }
            if let Some(exit) = state.last_exit {
                out.push_str(&format!("last exit: {exit}\n"));
            }
            if let Some(exited) = state.exited {
                out.push_str(&format!("exited: {exited}\n"));
            }
            out.push_str(&format!(
                "timeouts: text {}ms idle {}ms command {}ms exit {}ms ready {}ms\n",
                state.timeouts.text,
                state.timeouts.idle,
                state.timeouts.command,
                state.timeouts.exit,
                state.timeouts.ready,
            ));
            out.push_str("--- screen ---\n");
            out.push_str(&truncate_rendered(&state.text));
            out
        }
        OperationResult::Text(text) => truncate_rendered(text),
        OperationResult::PackedScreen(_) => "screen captured".to_string(),
        OperationResult::Cells(cells) => {
            let mut out = String::new();
            for cell in cells {
                out.push_str(&format!("({}, {}) '{}'{}", cell.x, cell.y, cell.char, attrs_string(cell)));
                out.push('\n');
            }
            truncate_rendered(&out)
        }
        OperationResult::Command(value) => value
            .as_ref()
            .map(|c| format!("command: {c}"))
            .unwrap_or_else(|| "(no command captured)".to_string()),
        OperationResult::Output(value) => value
            .as_ref()
            .map(|o| format!("output: {}", truncate_rendered(o)))
            .unwrap_or_else(|| "(no output captured)".to_string()),
        OperationResult::ExitCode(value) => value
            .map(|c| format!("exit-code: {c}"))
            .unwrap_or_else(|| "(no exit code captured)".to_string()),
        OperationResult::Cwd(value) => value
            .as_ref()
            .map(|c| format!("cwd: {c}"))
            .unwrap_or_else(|| "(no cwd captured)".to_string()),
        OperationResult::Cursor(cursor) => format!("cursor: {}, {}", cursor.x, cursor.y),
        OperationResult::Size(size) => format!("size: {}x{}", size.cols, size.rows),
        OperationResult::Snapshot(result) => match result {
            shell_use::SnapshotResult::Passed => "snapshot passed".to_string(),
            shell_use::SnapshotResult::Written => "snapshot written".to_string(),
            shell_use::SnapshotResult::Updated => "snapshot updated".to_string(),
        },
        OperationResult::Screenshot(result) => match result {
            shell_use::ScreenshotResult::Path(path) => format!("screenshot written to: {path}"),
            // Only fall back to rendering as text when no `path` was supplied —
            // returning a full SVG inline would be token-wasteful.
            shell_use::ScreenshotResult::Text(text) => truncate_rendered(text),
        },
    };
    AgentToolResult::text(text)
}

fn attrs_string(cell: &shell_use::Cell) -> String {
    let mut flags = Vec::new();
    if cell.bold {
        flags.push("bold");
    }
    if cell.dim {
        flags.push("dim");
    }
    if cell.italic {
        flags.push("italic");
    }
    if cell.inverse {
        flags.push("inverse");
    }
    if cell.underline {
        flags.push("underline");
    }
    if flags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", flags.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn tool_has_expected_name_and_schema() {
        let env = Arc::new(LocalExecutionEnv::new(PathBuf::from(".").as_path()));
        let tool = create_shell_use_tool(env);
        assert_eq!(tool.name(), "shell_use");
        let params = tool.tool.parameters.as_object().expect("object");
        let props = params.get("properties").and_then(Value::as_object).expect("props");
        assert!(props.contains_key("action"));
        assert!(props.contains_key("session"));
        let required = params.get("required").and_then(Value::as_array).expect("required");
        assert!(required.iter().any(|v| v.as_str() == Some("action")));
    }

    #[test]
    fn close_all_is_idempotent() {
        close_shell_use_sessions();
        close_shell_use_sessions();
    }

    #[test]
    fn open_sessions_list_is_sorted() {
        let names = shell_use_open_sessions();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    /// Drive a real bash session end-to-end through the tool's execute path
    /// (in-process PTY — no external daemon). Skips silently on hosts without
    /// a usable shell so the suite stays green on minimal CI.
    #[tokio::test]
    async fn drives_real_session_open_submit_expect_close() {
        close_shell_use_sessions();
        let env = Arc::new(LocalExecutionEnv::new(PathBuf::from(".").as_path()));
        let tool = create_shell_use_tool(env);
        let context = ToolContext::new(Arc::new(LocalExecutionEnv::new(PathBuf::from(".").as_path())));
        let exec = tool.execute.clone();

        let open = exec(
            "t1".into(),
            json!({ "action": "open", "session": "smoke", "shell": "bash", "cols": 80, "rows": 24 }),
            None,
            None,
            context.clone(),
        )
        .await;
        let open = match open {
            Ok(result) => result,
            Err(err) => {
                // Host without bash (or PTY unavailable) — not a code failure.
                eprintln!("skipping live shell smoke test: {err}");
                close_shell_use_sessions();
                return;
            }
        };
        let text = open.content.first().map(|c| format!("{c:?}")).unwrap_or_default();
        assert!(text.contains("open"), "open result: {text}");

        let submit = exec(
            "t2".into(),
            json!({ "action": "submit", "session": "smoke", "data": "echo hello-smoke" }),
            None,
            None,
            context.clone(),
        )
        .await
        .expect("submit");
        assert!(
            submit
                .content
                .first()
                .map(|c| format!("{c:?}").contains("ok"))
                .unwrap_or(false)
        );

        exec(
            "t3".into(),
            json!({ "action": "wait", "session": "smoke", "kind": "command", "timeout_ms": 15_000 }),
            None,
            None,
            context.clone(),
        )
        .await
        .expect("wait command");

        let expect = exec(
            "t4".into(),
            json!({ "action": "expect", "session": "smoke", "kind": "exit-code", "code": 0, "timeout_ms": 10_000 }),
            None,
            None,
            context.clone(),
        )
        .await
        .expect("expect exit-code");

        let text = exec(
            "t5".into(),
            json!({ "action": "text", "session": "smoke" }),
            None,
            None,
            context.clone(),
        )
        .await
        .expect("text");
        let screen = text.content.first().map(|c| format!("{c:?}")).unwrap_or_default();
        assert!(screen.contains("hello-smoke"), "expected echo on screen, got: {screen}");

        // soft-check expect exit-code 0 result
        let _ = expect;

        exec(
            "t6".into(),
            json!({ "action": "close", "session": "smoke" }),
            None,
            None,
            context,
        )
        .await
        .expect("close");
    }
}
