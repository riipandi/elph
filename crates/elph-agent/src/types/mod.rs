//! Core agent types — elph-agent module.

/// Host product name and env-var prefix for this runtime (not process-global).
///
/// `env_prefix` `MYAPP` → `MYAPP_PROMPT_ENCODING`, `MYAPP_AUTH_KEY`, `MYAPP_DATA_DIR`.
/// Default is `app_name = "elph-agent"`, `env_prefix = "ELPH"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostIdentity {
    pub app_name: String,
    pub env_prefix: String,
}

impl Default for HostIdentity {
    fn default() -> Self {
        Self {
            app_name: "elph-agent".to_string(),
            env_prefix: "ELPH".to_string(),
        }
    }
}

impl HostIdentity {
    pub fn new(app_name: impl Into<String>, env_prefix: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            env_prefix: env_prefix.into(),
        }
    }

    pub fn env_key(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.env_prefix)
    }
}

pub mod enums;

pub use enums::{AgentThinkingLevel, QueueMode, ToolExecutionMode};

/// Out-of-band agent-loop failure (`prompt` / `continue_run` / `reset`).
/// Stream token errors stay in-band on [`elph_ai::StopReason`].
#[derive(Debug)]
pub struct AgentError {
    pub code: AgentErrorCode,
    pub message: String,
    pub source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

/// Class of an [`AgentError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentErrorCode {
    /// Another run is already in flight.
    Busy,
    /// Transcript / queue state cannot continue.
    InvalidState,
    /// The inner loop or stream setup failed.
    Loop,
}

impl AgentError {
    pub fn new(code: AgentErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn busy(message: impl Into<String>) -> Self {
        Self::new(AgentErrorCode::Busy, message)
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::new(AgentErrorCode::InvalidState, message)
    }

    pub fn loop_failed(message: impl Into<String>) -> Self {
        Self::new(AgentErrorCode::Loop, message)
    }
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for AgentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|e| e as _)
    }
}

// Re-export from domain modules for a unified public API surface.
pub use crate::messages::types::assistant_message_to_agent;
pub use crate::messages::types::extract_tool_calls;
pub use crate::messages::types::llm_message_to_agent;
pub use crate::messages::types::tool_result_to_agent;
pub use crate::messages::types::{AgentMessage, CustomAgentMessage};
pub use crate::runtime::loop_config::AfterToolCallContext;
pub use crate::runtime::loop_config::AfterToolCallFn;
pub use crate::runtime::loop_config::AfterToolCallResult;
pub use crate::runtime::loop_config::AgentContext;
pub use crate::runtime::loop_config::AgentEvent;
pub use crate::runtime::loop_config::AgentLoopConfig;
pub use crate::runtime::loop_config::AgentLoopTurnUpdate;
pub use crate::runtime::loop_config::AgentState;
pub use crate::runtime::loop_config::BeforeToolCallContext;
pub use crate::runtime::loop_config::BeforeToolCallFn;
pub use crate::runtime::loop_config::BeforeToolCallResult;
pub use crate::runtime::loop_config::ConvertToLlmFn;
pub use crate::runtime::loop_config::GetApiKeyFn;
pub use crate::runtime::loop_config::GetQueuedMessagesFn;
pub use crate::runtime::loop_config::PrepareNextTurnContext;
pub use crate::runtime::loop_config::PrepareNextTurnFn;
pub use crate::runtime::loop_config::ShouldStopAfterTurnContext;
pub use crate::runtime::loop_config::ShouldStopAfterTurnFn;
pub use crate::runtime::loop_config::StreamFn;
pub use crate::runtime::loop_config::TransformContextFn;
pub use crate::tools::types::AgentTool;
pub use crate::tools::types::AgentToolCall;
pub use crate::tools::types::AgentToolResult;
pub use crate::tools::types::ToolContext;
pub use crate::tools::types::ToolError;
pub use crate::tools::types::ToolExecuteFn;
pub use crate::tools::types::ToolResultContent;
pub use crate::tools::types::ToolUpdateCallback;
