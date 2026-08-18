//! Core types for elph-ai: messages, models, stream options, and host identity.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::utils::event_stream::AssistantMessageEventStream;

pub type Api = String;
pub type ProviderId = String;
pub type ImagesApi = String;
pub type ImagesProviderId = String;
pub type ProviderEnv = HashMap<String, String>;
pub type ProviderHeaders = HashMap<String, Option<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelThinkingLevel {
    Off,
    Level(ThinkingLevel),
}

pub type ThinkingLevelMap = HashMap<String, Option<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheRetention {
    None,
    Short,
    Long,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    Sse,
    Websocket,
    #[serde(rename = "websocket-cached")]
    WebsocketCached,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkingBudgets {
    pub minimal: Option<u32>,
    pub low: Option<u32>,
    pub medium: Option<u32>,
    pub high: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
}

/// Host product identity used for provider headers and prefixed environment keys.
///
/// Defaults keep the Elph product names (`product = "elph"`, `env_prefix = "ELPH"`).
/// Third-party hosts should set this on [`crate::CreateModelsOptions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    /// Sent as Codex `originator`, xAI `referrer`, and similar client tags.
    pub product: String,
    /// Prefix for process env keys (`CACHE_RETENTION`, `GITHUB_HOST`, rate-limit vars).
    pub env_prefix: String,
}

impl Default for ClientIdentity {
    fn default() -> Self {
        Self {
            product: "elph".to_string(),
            env_prefix: "ELPH".to_string(),
        }
    }
}

impl ClientIdentity {
    pub fn new(product: impl Into<String>, env_prefix: impl Into<String>) -> Self {
        Self {
            product: product.into(),
            env_prefix: env_prefix.into(),
        }
    }

    pub fn env_key(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.env_prefix)
    }
}

#[derive(Clone, Default)]
pub struct StreamOptions {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub api_key: Option<String>,
    pub transport: Option<Transport>,
    pub cache_retention: Option<CacheRetention>,
    pub session_id: Option<String>,
    pub headers: Option<ProviderHeaders>,
    pub timeout_ms: Option<u64>,
    pub websocket_connect_timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_retry_delay_ms: Option<u64>,
    pub metadata: Option<HashMap<String, Value>>,
    pub env: Option<ProviderEnv>,
    pub on_payload: Option<OnPayloadCallback>,
    pub on_response: Option<OnResponseCallback>,
    pub signal: Option<tokio_util::sync::CancellationToken>,
    /// Custom HTTP client for per-request fetch injection.
    /// When set, provider adapters use this client instead of building their own.
    pub client: Option<reqwest::Client>,
    /// Arbitrary sampling parameters (e.g. `top_p`, `top_k`, `min_p`, `repetition_penalty`)
    /// merged into OpenAI-compatible request bodies. Overrides any model-level defaults.
    pub sampling_params: Option<HashMap<String, Value>>,
    /// Host identity for this request. Filled from [`crate::CreateModelsOptions`] when unset.
    pub identity: Option<ClientIdentity>,
}

impl StreamOptions {
    pub fn identity_or_default(&self) -> ClientIdentity {
        self.identity.clone().unwrap_or_default()
    }
}

pub type OnPayloadCallback =
    Arc<dyn Fn(Value, Model) -> Pin<Box<dyn Future<Output = Option<Value>> + Send>> + Send + Sync>;
pub type OnResponseCallback =
    Arc<dyn Fn(ProviderResponse, Model) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct SimpleStreamOptions {
    pub base: StreamOptions,
    pub reasoning: Option<ThinkingLevel>,
    pub thinking_budgets: Option<ThinkingBudgets>,
}

impl SimpleStreamOptions {
    pub fn from_stream(options: StreamOptions) -> Self {
        Self {
            base: options,
            reasoning: None,
            thinking_budgets: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TextContent {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            kind: "text".to_string(),
            text: text.into(),
            text_signature: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ThinkingContent {
    #[serde(rename = "type")]
    pub kind: String,
    pub thinking: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redacted: Option<bool>,
}

impl ThinkingContent {
    pub fn new(thinking: impl Into<String>) -> Self {
        Self {
            kind: "thinking".to_string(),
            thinking: thinking.into(),
            thinking_signature: None,
            redacted: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImageContent {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: String,
    pub mime_type: String,
}

impl ImageContent {
    pub fn new(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            kind: "image".to_string(),
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    pub name: String,
    pub arguments: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            kind: "toolCall".to_string(),
            id: id.into(),
            name: name.into(),
            arguments,
            thought_signature: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_1h: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<u64>,
    pub total_tokens: u64,
    pub cost: UsageCost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    Stop,
    Length,
    #[serde(rename = "toolUse")]
    ToolUse,
    Error,
    Aborted,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum Message {
    User {
        content: UserContent,
        timestamp: i64,
    },
    Assistant(AssistantMessage),
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        content: Vec<ContentBlock>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        /// Names from `Context.tools` that became available after this result.
        /// Providers with native deferred tool loading use this as the load point.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        added_tool_names: Option<Vec<String>>,
        /// Optional usage metadata reported by the tool execution, if available.
        /// Hosts may attach this for cost tracking and diagnostics.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        is_error: bool,
        timestamp: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
}

fn assistant_role_default() -> String {
    "assistant".to_string()
}

/// Redacted provider/runtime diagnostic attached to an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessageDiagnostic {
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    #[serde(skip_serializing, default = "assistant_role_default")]
    pub role: String,
    pub content: Vec<AssistantContentBlock>,
    pub api: Api,
    pub provider: ProviderId,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    /// Redacted provider/runtime diagnostics for failures and recoveries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Vec<AssistantMessageDiagnostic>>,
    pub usage: Usage,
    pub stop_reason: StopReason,
    /// Stop reason known mid-stream, before the stream completes.
    /// Set when a provider emits a stop reason delta (e.g. Anthropic `delta/stop_reason`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_stop_reason: Option<StopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssistantContentBlock {
    Text(TextContent),
    Thinking(ThinkingContent),
    ToolCall(ToolCall),
}

impl AssistantMessage {
    pub fn empty(model: &Model) -> Self {
        Self {
            role: "assistant".to_string(),
            content: vec![],
            api: model.api.clone(),
            provider: model.provider.clone(),
            model: model.id.clone(),
            response_model: None,
            response_id: None,
            diagnostics: None,
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            pending_stop_reason: None,
            error_message: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// Configuration for constrained tool sampling.
///
/// When set on a `Tool`, the provider enforces the specified sampling constraint:
/// - `json_schema`: strict JSON Schema enforcement (`prefer` = best-effort, `require` = strict)
/// - `grammar`: provider-specific grammar variants (Lark, regex, etc.)
///
/// Ported from pi `ConstrainedSamplingConfig`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstrainedSamplingConfig {
    JsonSchema {
        /// Whether to prefer or require strict JSON Schema enforcement.
        strict: StrictMode,
    },
    Grammar {
        /// Grammar variants for provider-specific encodings.
        variants: GrammarVariants,
    },
}

/// Strictness level for JSON Schema constrained sampling.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrictMode {
    Prefer,
    Require,
}

/// Provider-specific grammar variants for constrained sampling.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct GrammarVariants {
    /// Lark grammar definition for OpenAI grammar tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lark: Option<String>,
    /// Regex pattern for constrained generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    /// Constrained sampling configuration.
    /// When `None`, no constraint is applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constrained_sampling: Option<ConstrainedSamplingConfig>,
}

impl Tool {
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            constrained_sampling: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Context {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Tool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSignatureV1 {
    pub v: u8,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantMessageEvent {
    Start {
        partial: AssistantMessage,
    },
    TextStart {
        content_index: usize,
        partial: AssistantMessage,
    },
    TextDelta {
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    TextEnd {
        content_index: usize,
        content: String,
        partial: AssistantMessage,
    },
    ThinkingStart {
        content_index: usize,
        partial: AssistantMessage,
    },
    ThinkingDelta {
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    ThinkingEnd {
        content_index: usize,
        content: String,
        partial: AssistantMessage,
    },
    ToolcallStart {
        content_index: usize,
        partial: AssistantMessage,
    },
    ToolcallDelta {
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    ToolcallEnd {
        content_index: usize,
        tool_call: ToolCall,
        partial: AssistantMessage,
    },
    Done {
        reason: StopReason,
        message: AssistantMessage,
    },
    Error {
        reason: StopReason,
        error: AssistantMessage,
    },
}

/// Session-affinity header format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionAffinityFormat {
    /// Sends `session_id`, `x-client-request-id`, and `x-session-affinity`.
    OpenAI,
    /// Sends `x-client-request-id` and `x-session-affinity` but no `session_id`.
    OpenAINoSession,
    /// Sends `x-session-id` for OpenRouter-compatible session affinity.
    OpenRouter,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAICompletionsCompat {
    pub supports_store: Option<bool>,
    pub supports_developer_role: Option<bool>,
    pub supports_reasoning_effort: Option<bool>,
    pub supports_usage_in_streaming: Option<bool>,
    pub max_tokens_field: Option<String>,
    pub requires_tool_result_name: Option<bool>,
    pub requires_assistant_after_tool_result: Option<bool>,
    pub requires_thinking_as_text: Option<bool>,
    pub requires_reasoning_content_on_assistant_messages: Option<bool>,
    pub thinking_format: Option<String>,
    pub zai_tool_stream: Option<bool>,
    pub supports_strict_mode: Option<bool>,
    /// Whether the provider supports OpenAI custom tools with Lark/regex grammar formats.
    pub supports_openai_grammar_tools: Option<bool>,
    pub cache_control_format: Option<String>,
    pub send_session_affinity_headers: Option<bool>,
    pub session_affinity_format: Option<SessionAffinityFormat>,
    pub supports_long_cache_retention: Option<bool>,
    /// Whether streamed responses include `finish_reason`. When false, the adapter infers
    /// `stop` or `toolUse` when the stream ends instead of erroring.
    pub supports_finish_reason: Option<bool>,
    /// Whether the provider supports top-level `thinking_token_budget` to cap reasoning tokens
    /// (e.g. vLLM). Reasoning and the answer share `max_tokens`, so without a budget a
    /// reasoning-heavy turn can emit no answer.
    pub supports_thinking_token_budget: Option<bool>,
    /// Arbitrary sampling parameters (top_p, top_k, min_p, repetition_penalty, ...) merged into
    /// OpenAI-compatible request bodies. Per-request `StreamOptions` values override these.
    pub sampling_params: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAIResponsesCompat {
    pub supports_developer_role: Option<bool>,
    pub send_session_id_header: Option<bool>,
    pub session_affinity_format: Option<SessionAffinityFormat>,
    pub supports_long_cache_retention: Option<bool>,
    /// Whether the model supports client-executed tool search for deferred tools.
    pub supports_tool_search: Option<bool>,
    /// Whether the provider supports strict JSON-schema function tools.
    pub supports_strict_mode: Option<bool>,
    /// Whether the provider supports OpenAI custom tools with Lark/regex grammar formats.
    pub supports_openai_grammar_tools: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnthropicMessagesCompat {
    pub supports_eager_tool_input_streaming: Option<bool>,
    pub supports_long_cache_retention: Option<bool>,
    pub send_session_affinity_headers: Option<bool>,
    pub supports_cache_control_on_tools: Option<bool>,
    pub supports_temperature: Option<bool>,
    pub force_adaptive_thinking: Option<bool>,
    pub allow_empty_signature: Option<bool>,
    /// Whether the provider supports deferred tools loaded by `tool_reference`.
    pub supports_tool_references: Option<bool>,
    /// Whether the provider supports Anthropic strict tool schemas.
    pub supports_strict_tools: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: Api,
    pub provider: ProviderId,
    pub base_url: String,
    pub reasoning: bool,
    pub thinking_level_map: Option<ThinkingLevelMap>,
    pub input: Vec<String>,
    pub cost: ModelCost,
    pub context_window: u32,
    pub max_tokens: u32,
    pub headers: Option<HashMap<String, String>>,
    pub openai_completions_compat: Option<OpenAICompletionsCompat>,
    pub openai_responses_compat: Option<OpenAIResponsesCompat>,
    pub anthropic_compat: Option<AnthropicMessagesCompat>,
}

/// Base token rates in USD per million tokens.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostRates {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

/// Request-wide pricing tier. Applies when total input usage exceeds the threshold.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostTier {
    /// Use this tier for requests whose total input usage exceeds this token count.
    pub input_tokens_above: u64,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    /// Request-wide pricing tiers. The highest matching input threshold applies to the full request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiers: Option<Vec<ModelCostTier>>,
}

impl ModelCost {
    pub fn flat(input: f64, output: f64, cache_read: f64, cache_write: f64) -> Self {
        Self {
            input,
            output,
            cache_read,
            cache_write,
            tiers: None,
        }
    }

    pub fn rates(&self) -> ModelCostRates {
        ModelCostRates {
            input: self.input,
            output: self.output,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
        }
    }
}

impl Default for ModelCost {
    fn default() -> Self {
        Self::flat(0.0, 0.0, 0.0, 0.0)
    }
}

#[derive(Debug, Clone)]
pub struct ImagesModel {
    pub id: String,
    pub name: String,
    pub api: ImagesApi,
    pub provider: ImagesProviderId,
    pub base_url: String,
    pub input: Vec<String>,
    pub output: Vec<String>,
    pub cost: ModelCost,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct ImagesContext {
    pub input: Vec<ContentBlock>,
}

#[derive(Clone)]
pub struct ImagesOptions {
    pub api_key: Option<String>,
    pub signal: Option<tokio_util::sync::CancellationToken>,
    pub env: Option<ProviderEnv>,
    pub headers: Option<ProviderHeaders>,
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub on_payload: Option<OnPayloadCallback>,
    pub on_response: Option<OnResponseCallback>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantImages {
    pub api: ImagesApi,
    pub provider: ImagesProviderId,
    pub model: String,
    pub output: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    pub stop_reason: StopReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub timestamp: i64,
}

/// Uniform stream contract for API implementation modules.
pub trait ProviderStreams: Send + Sync {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream;

    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream;
}

pub trait ProviderImages: Send + Sync {
    fn generate_images(
        &self,
        model: &ImagesModel,
        context: &ImagesContext,
        options: Option<ImagesOptions>,
    ) -> Pin<Box<dyn Future<Output = AssistantImages> + Send>>;
}

// Message helpers
impl Message {
    pub fn role(&self) -> &'static str {
        match self {
            Message::User { .. } => "user",
            Message::Assistant(_) => "assistant",
            Message::ToolResult { .. } => "toolResult",
        }
    }

    pub fn as_assistant(&self) -> Option<&AssistantMessage> {
        match self {
            Message::Assistant(m) => Some(m),
            _ => None,
        }
    }
}

impl AssistantContentBlock {
    pub fn is_text(&self) -> bool {
        matches!(self, AssistantContentBlock::Text(_))
    }

    pub fn is_thinking(&self) -> bool {
        matches!(self, AssistantContentBlock::Thinking(_))
    }

    pub fn is_tool_call(&self) -> bool {
        matches!(self, AssistantContentBlock::ToolCall(_))
    }
}
