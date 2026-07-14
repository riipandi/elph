//! Agent → TUI event bridge.

/// Live UI events emitted while an agent run is in progress.
#[derive(Debug)]
pub enum AgentUiEvent {
    Status(String),
    TextDelta(String),
    ThinkingDelta(String),
    ToolStart {
        id: String,
        name: String,
        args_summary: String,
    },
    ToolUpdate {
        id: String,
        output: String,
    },
    ToolEnd {
        id: String,
        is_error: bool,
        output: String,
    },
    RunCompleted {
        elapsed_secs: f64,
    },
    PlanConfirmationRequired(PlanConfirmationRequest),
    ToolApprovalRequired(ToolApprovalRequest),
    SubagentStatus {
        agent_id: String,
        agent_path: String,
        message: String,
    },
    GoalUpdated {
        objective: Option<String>,
        status: Option<String>,
    },
    UserQuestionRequired(UserQuestionRequest),
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
    Approve,
    Reject,
    AllowSession,
}

/// Ask-user question presented by the `ask_user_question` tool.
#[derive(Debug)]
pub struct UserQuestionRequest {
    /// The question text to display.
    pub question: String,
    /// Optional list of selectable options (select mode).
    pub options: Option<Vec<UserQuestionOption>>,
    /// Optional default value.
    pub default: Option<String>,
    /// Channel to send the user's answer back to the tool.
    pub response_tx: tokio::sync::oneshot::Sender<String>,
}

#[derive(Debug, Clone)]
pub struct UserQuestionOption {
    pub value: String,
    pub label: String,
}
