//! Agent → TUI event bridge (streaming text, tools, status).

use tokio::sync::oneshot;

/// Kind of interactive prompt requested by an ask_* tool.
#[derive(Debug, Clone)]
pub enum AskUserKind {
    Text { default: Option<String> },
    Select { options: Vec<String>, default_index: usize },
    Confirm { default: bool },
}

/// User response to an ask_* prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskUserResponse {
    Answered(String),
    Cancelled,
}

/// Live UI events emitted while an agent run is in progress.
#[derive(Debug)]
pub enum AgentUiEvent {
    /// Init/update/chat command header.
    CommandStart {
        command: String,
        provider: String,
        model: String,
    },
    /// Command finished with a user-facing summary.
    CommandComplete { message: String, success: bool },
    /// Short status line (auth, noop skip, progress).
    Status(String),
    /// Incremental assistant text.
    TextDelta(String),
    /// Incremental thinking text (verbose mode).
    ThinkingDelta(String),
    /// Tool invocation started.
    ToolStart {
        id: String,
        name: String,
        args_summary: String,
    },
    /// Streaming/partial tool output.
    ToolUpdate { id: String, output: String },
    /// Tool invocation finished.
    ToolEnd { id: String, is_error: bool, output: String },
    /// Agent run finished.
    RunCompleted { elapsed_secs: f64 },
    /// Ask tool waiting for user input (TUI prompts or modals).
    AskUserRequired {
        tool_call_id: String,
        tool_name: String,
        question: String,
        kind: AskUserKind,
        response_tx: oneshot::Sender<AskUserResponse>,
    },
}
