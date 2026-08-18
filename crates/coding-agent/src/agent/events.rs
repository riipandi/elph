//! Agent → TUI event bridge.

use elph_agent::{TodoItem, TurnUsage};

/// Recovery prompt submitted instead of re-sending the original text when a transient
/// stream/provider error interrupts a turn. The model resumes from the last persisted
/// state (any tool results already in the conversation stay in context) instead of
/// re-running the whole task, so completed tool calls are not duplicated.
///
/// Shared with the TUI shell so the recovery prompt is recognized and rendered as a
/// slim status label instead of a user bubble card in the transcript.
pub const RETRY_CONTINUE_PROMPT: &str = "Continue: the previous response was interrupted by a transient stream error. \
     Resume from where you left off and finish the task. Do not repeat tool calls or \
     actions that already succeeded.";

/// Slim sticky meta label rendered in the transcript for the retry/continue prompt.
///
/// Used by the live shell applier and by resume reconstruction so the recovery
/// prompt shows as one quiet status line instead of a giant user prompt card.
/// The label must stay identical in both paths so a live `continue` notice and a
/// resumed one render the same.
pub const CONTINUE_META_LABEL: &str = "Continuing tasks…";

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
            Self::Pending => "starting",
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
    /// Recovery prompt the shell re-submits on Ctrl+R after a transient stream/API error.
    /// Carries a "Continue"-style message (not the original prompt) so completed tool work
    /// is not duplicated. Shell stores this for the Ctrl+R retry key / error-card affordance.
    /// The prompt itself is never shown as a user card — the shell renders a
    /// `Continuing tasks…` meta label instead.
    RetryablePrompt(String),
    /// The session is automatically retrying an interrupted turn. The shell shows a
    /// spinner + "Retrying…" activity label (`attempt` is 1-based) instead of an idle bar.
    Retrying {
        attempt: u32,
    },
    /// Durable transcript notice (conflicts, reload details). Always **appends** a Meta
    /// card — unlike [`Self::Status`], it is not collapsed into the previous Meta line.
    TranscriptNotice(String),
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
        /// Provider-reported usage for the turn (input/output/cache), when available.
        usage: Option<TurnUsage>,
        /// Provider id the turn ran on (e.g. `openai`), when known.
        provider_id: Option<String>,
        /// Model id the turn ran on, when known.
        model_id: Option<String>,
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
    /// `/aside` side question started (panel / status).
    AsideStarted {
        request_id: u64,
        question: String,
    },
    /// `/aside` answer ready — shell opens a scroll dialog (not session history).
    AsideFinished {
        request_id: u64,
        question: String,
        answer: String,
    },
    /// `/aside` failed.
    AsideFailed {
        request_id: u64,
        error: String,
    },
    /// A worker message was received (threaded, via worker_send/reply/ask).
    /// The shell shows the inbox badge and stores the message for the worker chat.
    WorkerInboxReceived {
        msg_id: String,
        from_worker: String,
        from_worker_id: String,
        text: String,
        created_at: String,
    },
    /// A worker message was sent by this session (worker_send/reply/ask).
    WorkerInboxSent {
        msg_id: String,
        to_worker: String,
        to_worker_id: String,
        text: String,
        created_at: String,
    },
    /// The worker inbox changed in a way the shell should re-read.
    WorkerInboxUpdated,
    /// Todo list updated by the agent (todo_write tool call).
    TodoUpdated {
        items: Vec<TodoItem>,
    },
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
    /// Plan mode: Allow once / Deny only — no session or all-tools grant.
    pub once_only: bool,
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
