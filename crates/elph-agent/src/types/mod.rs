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
pub use crate::tools::types::ToolExecuteFn;
pub use crate::tools::types::ToolResultContent;
pub use crate::tools::types::ToolUpdateCallback;
