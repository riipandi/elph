//! Core agent types — ported from pi-agent `types.ts`.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::utils::event_stream::AssistantMessageEventStream;
use elph_ai::{
    AssistantMessage, AssistantMessageEvent, Context, ImageContent, Message, Model, SimpleStreamOptions, TextContent,
    ThinkingLevel, Tool, ToolCall,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub type StreamFn =
    Arc<dyn Fn(&Model, &Context, Option<SimpleStreamOptions>) -> AssistantMessageEventStream + Send + Sync>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolExecutionMode {
    Sequential,
    #[default]
    Parallel,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QueueMode {
    All,
    #[default]
    #[serde(rename = "one-at-a-time")]
    OneAtATime,
}

/// Thinking level including harness-only `Off`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl AgentThinkingLevel {
    pub fn to_stream_reasoning(self) -> Option<ThinkingLevel> {
        match self {
            Self::Off => None,
            Self::Minimal => Some(ThinkingLevel::Minimal),
            Self::Low => Some(ThinkingLevel::Low),
            Self::Medium => Some(ThinkingLevel::Medium),
            Self::High => Some(ThinkingLevel::High),
            Self::Xhigh => Some(ThinkingLevel::Xhigh),
        }
    }
}

/// App-level transcript message (LLM messages + custom roles).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentMessage {
    Llm(Box<Message>),
    Custom(CustomAgentMessage),
}

impl AgentMessage {
    pub fn role(&self) -> &str {
        match self {
            Self::Llm(m) => m.role(),
            Self::Custom(c) => c.role(),
        }
    }

    pub fn as_llm(&self) -> Option<&Message> {
        match self {
            Self::Llm(m) => Some(m.as_ref()),
            _ => None,
        }
    }

    pub fn into_llm(self) -> Option<Message> {
        match self {
            Self::Llm(m) => Some(*m),
            _ => None,
        }
    }
}

/// Custom harness message roles (extended in `messages` module).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum CustomAgentMessage {
    BashExecution {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<String>,
        timestamp: i64,
    },
    BranchSummary {
        summary: String,
        #[serde(rename = "fromId")]
        from_id: String,
        timestamp: i64,
    },
    CompactionSummary {
        summary: String,
        #[serde(rename = "tokensBefore")]
        tokens_before: u64,
        timestamp: i64,
    },
    Custom {
        #[serde(rename = "type")]
        kind: String,
        content: Value,
        #[serde(default)]
        display: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        timestamp: i64,
    },
}

impl CustomAgentMessage {
    pub fn role(&self) -> &str {
        match self {
            Self::BashExecution { .. } => "bashExecution",
            Self::BranchSummary { .. } => "branchSummary",
            Self::CompactionSummary { .. } => "compactionSummary",
            Self::Custom { .. } => "custom",
        }
    }
}

pub type AgentToolCall = ToolCall;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub content: Vec<ToolResultContent>,
    pub details: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(TextContent),
    Image(ImageContent),
}

impl AgentToolResult {
    pub fn text(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolResultContent::Text(TextContent::new(message))],
            details: Value::Object(Default::default()),
            terminate: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::text(message)
    }
}

pub type ToolUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>;

pub type ToolExecuteFn = Arc<
    dyn Fn(
            String,
            Value,
            Option<CancellationToken>,
            Option<ToolUpdateCallback>,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct AgentTool {
    pub tool: Tool,
    pub label: String,
    pub execution_mode: Option<ToolExecutionMode>,
    pub prepare_arguments: Option<Arc<dyn Fn(Value) -> Value + Send + Sync>>,
    pub execute: ToolExecuteFn,
}

impl AgentTool {
    pub fn name(&self) -> &str {
        &self.tool.name
    }
}

#[derive(Clone)]
pub struct AgentContext {
    pub system_prompt: String,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<AgentTool>,
}

#[derive(Debug, Clone)]
pub struct BeforeToolCallResult {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AfterToolCallResult {
    pub content: Option<Vec<ToolResultContent>>,
    pub details: Option<Value>,
    pub is_error: Option<bool>,
    pub terminate: Option<bool>,
}

#[derive(Clone)]
pub struct BeforeToolCallContext {
    pub assistant_message: AssistantMessage,
    pub tool_call: AgentToolCall,
    pub args: Value,
    pub context: AgentContext,
}

#[derive(Clone)]
pub struct AfterToolCallContext {
    pub assistant_message: AssistantMessage,
    pub tool_call: AgentToolCall,
    pub args: Value,
    pub result: AgentToolResult,
    pub is_error: bool,
    pub context: AgentContext,
}

#[derive(Clone)]
pub struct ShouldStopAfterTurnContext {
    pub message: AssistantMessage,
    pub tool_results: Vec<Message>,
    pub context: AgentContext,
    pub new_messages: Vec<AgentMessage>,
}

#[derive(Clone)]
pub struct AgentLoopTurnUpdate {
    pub context: Option<AgentContext>,
    pub model: Option<Model>,
    pub thinking_level: Option<AgentThinkingLevel>,
}

pub type PrepareNextTurnContext = ShouldStopAfterTurnContext;

pub type ConvertToLlmFn =
    Arc<dyn Fn(Vec<AgentMessage>) -> Pin<Box<dyn Future<Output = Vec<Message>> + Send>> + Send + Sync>;

pub type TransformContextFn = Arc<
    dyn Fn(
            Vec<AgentMessage>,
            Option<CancellationToken>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<AgentMessage>, String>> + Send>>
        + Send
        + Sync,
>;

pub type GetApiKeyFn = Arc<dyn Fn(&str) -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync>;

pub type ShouldStopAfterTurnFn =
    Arc<dyn Fn(ShouldStopAfterTurnContext) -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

pub type PrepareNextTurnFn = Arc<
    dyn Fn(PrepareNextTurnContext) -> Pin<Box<dyn Future<Output = Option<AgentLoopTurnUpdate>> + Send>> + Send + Sync,
>;

pub type GetQueuedMessagesFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Vec<AgentMessage>> + Send>> + Send + Sync>;

pub type BeforeToolCallFn = Arc<
    dyn Fn(
            BeforeToolCallContext,
            Option<CancellationToken>,
        ) -> Pin<Box<dyn Future<Output = Option<BeforeToolCallResult>> + Send>>
        + Send
        + Sync,
>;

pub type AfterToolCallFn = Arc<
    dyn Fn(
            AfterToolCallContext,
            Option<CancellationToken>,
        ) -> Pin<Box<dyn Future<Output = Option<AfterToolCallResult>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct AgentLoopConfig {
    pub model: Model,
    pub stream_options: SimpleStreamOptions,
    pub convert_to_llm: ConvertToLlmFn,
    pub transform_context: Option<TransformContextFn>,
    pub get_api_key: Option<GetApiKeyFn>,
    pub should_stop_after_turn: Option<ShouldStopAfterTurnFn>,
    pub prepare_next_turn: Option<PrepareNextTurnFn>,
    pub get_steering_messages: Option<GetQueuedMessagesFn>,
    pub get_follow_up_messages: Option<GetQueuedMessagesFn>,
    pub tool_execution: ToolExecutionMode,
    pub before_tool_call: Option<BeforeToolCallFn>,
    pub after_tool_call: Option<AfterToolCallFn>,
    pub stream_fn: Option<StreamFn>,
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    AgentStart,
    AgentEnd {
        messages: Vec<AgentMessage>,
    },
    TurnStart,
    TurnEnd {
        message: AgentMessage,
        tool_results: Vec<Message>,
    },
    MessageStart {
        message: AgentMessage,
    },
    MessageUpdate {
        message: AgentMessage,
        assistant_message_event: Box<AssistantMessageEvent>,
    },
    MessageEnd {
        message: AgentMessage,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: Value,
    },
    ToolExecutionUpdate {
        tool_call_id: String,
        tool_name: String,
        args: Value,
        partial_result: AgentToolResult,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: AgentToolResult,
        is_error: bool,
    },
}

/// Public agent state snapshot.
#[derive(Clone)]
pub struct AgentState {
    pub system_prompt: String,
    pub model: Model,
    pub thinking_level: AgentThinkingLevel,
    pub tools: Vec<AgentTool>,
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub streaming_message: Option<AgentMessage>,
    pub pending_tool_calls: HashSet<String>,
    pub error_message: Option<String>,
}

pub fn assistant_message_to_agent(message: AssistantMessage) -> AgentMessage {
    AgentMessage::Llm(Box::new(Message::Assistant(message)))
}

pub fn tool_result_to_agent(message: Message) -> AgentMessage {
    AgentMessage::Llm(Box::new(message))
}

pub fn llm_message_to_agent(message: Message) -> AgentMessage {
    AgentMessage::Llm(Box::new(message))
}

pub fn extract_tool_calls(message: &AssistantMessage) -> Vec<&ToolCall> {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            elph_ai::AssistantContentBlock::ToolCall(tc) => Some(tc),
            _ => None,
        })
        .collect()
}
