//! Agent → TUI event bridge.

/// Lifecycle phase for subagent UI (maps to process glyphs / status colors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentUiPhase {
    Pending,
    Running,
    Idle,
    Error,
    Done,
}

impl SubagentUiPhase {
    /// Plain-language status word for a11y (not color-only).
    pub fn as_word(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Idle => "idle",
            Self::Error => "error",
            Self::Done => "done",
        }
    }
}

/// Kind of prompt sitting in the agent harness queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuedPromptKind {
    /// Delivered after the current agent work finishes (Enter while busy).
    FollowUp,
    /// Mid-turn interjection / steering (Ctrl+Enter).
    Steer,
}

/// One queued user prompt for StatusRow / queue manager UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedPromptItem {
    /// 1-based index within the combined display list (follow-ups first, then steer).
    pub seq: u32,
    pub kind: QueuedPromptKind,
    /// Index into the harness queue of this kind (for cancel/edit).
    pub kind_index: usize,
    pub text: String,
}

/// Live UI events emitted while an agent run is in progress.
#[derive(Debug)]
pub enum AgentUiEvent {
    Status(String),
    /// Result of a `/memory` command — the shell opens a ScrollTextDialog.
    MemoryResult(String),
    TextDelta(String),
    ThinkingDelta(String),
    ToolStart {
        id: String,
        name: String,
        args_summary: String,
        /// User-initiated shell execution (`!`/`!!`) — output renders unlimited.
        user_shell: bool,
    },
    ToolUpdate {
        id: String,
        output: String,
    },
    ToolEnd {
        id: String,
        is_error: bool,
        output: String,
        /// Structured tool-result metadata (old/new content for edit_file, etc.).
        details: serde_json::Value,
    },
    RunCompleted {
        elapsed_secs: f64,
    },
    /// Harness steer/follow-up queue snapshot (after enqueue, drain, cancel, or abort).
    QueueUpdate {
        items: Vec<QueuedPromptItem>,
    },
    /// A user message was committed by the agent loop (initial prompt, steer, or drained follow-up).
    /// The TUI may skip rendering if it already echoed the prompt (idle submit / Ctrl+Enter).
    UserPromptCommitted {
        text: String,
    },
    PlanConfirmationRequired(PlanConfirmationRequest),
    ToolApprovalRequired(ToolApprovalRequest),
    /// Live subagent lifecycle / tool activity (upserted per agent in the transcript).
    SubagentStatus {
        agent_id: String,
        agent_path: String,
        /// Human task label when available (prefer over raw id).
        task_name: String,
        phase: SubagentUiPhase,
        /// Short action (tool name, "done", error detail, …).
        message: String,
        /// Model id shown in brackets, e.g. `claude-sonnet-4-20250514`.
        model: String,
    },
    /// Subagent output text (accumulated deltas, tool results, completion markers).
    SubagentOutput {
        agent_id: String,
        content: String,
    },
    GoalUpdated {
        objective: Option<String>,
        status: Option<String>,
    },
    UserQuestionRequired(UserQuestionRequest),
    /// Agent requests a mode change (Ask/Plan → Build/Brave).
    ModeChangeRequired(ModeChangeRequest),
}

#[derive(Debug)]
pub struct ModeChangeRequest {
    pub target_mode: String,
    pub reason: String,
    pub response_tx: tokio::sync::oneshot::Sender<String>,
}

#[derive(Debug, Clone)]
pub struct PlanConfirmationRequest {
    pub plan_id: String,
    pub plan_text: String,
}

#[derive(Debug)]
pub struct ToolApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args_summary: String,
    pub response_tx: tokio::sync::oneshot::Sender<ToolApprovalChoice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolApprovalChoice {
    /// Run this tool call only; ask again next time.
    Approve,
    /// Deny this tool call; ask again next time.
    Reject,
    /// Allow this tool name for the rest of the session.
    AllowSession,
    /// Allow every tool that requires approval for the rest of the session.
    AllowAllTools,
}

/// One step in a single- or multi-step ask-user flow.
#[derive(Debug, Clone)]
pub struct UserQuestionStep {
    pub id: String,
    pub question: String,
    pub options: Option<Vec<UserQuestionOption>>,
    pub allow_multiple: bool,
    pub allow_custom: bool,
    pub custom_label: String,
    pub default: Option<String>,
    /// When false, the user may skip this step with Esc (empty answer).
    pub required: bool,
    /// Minimum length for free-text answers (ignored for select / confirm steps).
    pub min_length: Option<usize>,
    /// Optional regex pattern for free-text answers.
    pub pattern: Option<String>,
    /// Short label shown in the multi-step header tab row.
    pub tab_label: Option<String>,
}

/// Ask-user session presented by the `ask_user_question` tool.
#[derive(Debug)]
pub struct UserQuestionRequest {
    pub steps: Vec<UserQuestionStep>,
    pub response_tx: tokio::sync::oneshot::Sender<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UserQuestionOption {
    pub value: String,
    pub label: String,
    /// Optional dimmed detail shown below the label in the question dialog.
    pub hint: Option<String>,
}
