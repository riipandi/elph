//! Core agent types — elph-agent module.

mod enums;
mod loop_config;
mod messages;
mod tools;

pub use enums::{AgentThinkingLevel, QueueMode, ToolExecutionMode};
pub use loop_config::{
    AfterToolCallContext, AfterToolCallFn, AfterToolCallResult, AgentContext, AgentEvent, AgentLoopConfig,
    AgentLoopTurnUpdate, AgentState, BeforeToolCallContext, BeforeToolCallFn, BeforeToolCallResult, ConvertToLlmFn,
    GetApiKeyFn, GetQueuedMessagesFn, PrepareNextTurnContext, PrepareNextTurnFn, PrepareNextTurnLegacyFn,
    ShouldStopAfterTurnContext, ShouldStopAfterTurnFn, StreamFn, TransformContextFn,
};
pub use messages::{
    AgentMessage, CustomAgentMessage, assistant_message_to_agent, extract_tool_calls, llm_message_to_agent,
    tool_result_to_agent,
};
pub use tools::{AgentTool, AgentToolCall, AgentToolResult, ToolExecuteFn, ToolResultContent, ToolUpdateCallback};
