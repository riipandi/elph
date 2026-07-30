//! Core agent types — elph-agent module.
//!
//! Global enums live here; domain-specific types have been distributed:
//! - loop config types → `crate::runtime::loop_config`
//! - message types → `crate::messages::types`
//! - tool types → `crate::tools::types`

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
