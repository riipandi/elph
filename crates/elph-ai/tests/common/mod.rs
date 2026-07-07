use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use elph_ai::auth::AuthContext;
use elph_ai::types::{
    AnthropicMessagesCompat, AssistantMessage, CacheRetention, Context, Message, Model, ModelCost,
    OpenAICompletionsCompat, OpenAIResponsesCompat, StopReason, StreamOptions, Usage, UserContent,
};

pub struct FakeAuthContext {
    pub env: HashMap<String, String>,
    pub files: HashSet<String>,
}

impl FakeAuthContext {
    pub fn new(env: HashMap<String, String>, files: Vec<&str>) -> Self {
        Self {
            env,
            files: files.into_iter().map(str::to_string).collect(),
        }
    }
}

#[async_trait::async_trait]
impl AuthContext for FakeAuthContext {
    async fn env(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    async fn file_exists(&self, path: &str) -> bool {
        self.files.contains(path)
    }
}

pub fn fake_auth_context(env: HashMap<String, String>, files: Vec<&str>) -> Arc<dyn AuthContext> {
    Arc::new(FakeAuthContext::new(env, files))
}

pub fn error_assistant_message(error_message: impl Into<String>) -> AssistantMessage {
    AssistantMessage {
        role: "assistant".to_string(),
        content: vec![],
        api: "openai-completions".to_string(),
        provider: "test".to_string(),
        model: "test-model".to_string(),
        response_model: None,
        response_id: None,
        usage: Usage::default(),
        stop_reason: StopReason::Error,
        error_message: Some(error_message.into()),
        timestamp: 0,
    }
}

pub fn sample_user_context(system_prompt: Option<&str>) -> Context {
    Context {
        system_prompt: system_prompt.map(str::to_string),
        messages: vec![Message::User {
            content: UserContent::Text("Hello".to_string()),
            timestamp: 0,
        }],
        tools: None,
    }
}

pub fn completions_proxy_model(compat: Option<OpenAICompletionsCompat>) -> Model {
    Model {
        id: "test-model".to_string(),
        name: "Test Model".to_string(),
        api: "openai-completions".to_string(),
        provider: "test-openai-completions".to_string(),
        base_url: "https://my-proxy.example.com/v1".to_string(),
        reasoning: false,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 128_000,
        max_tokens: 4096,
        headers: None,
        openai_completions_compat: compat,
        openai_responses_compat: None,
        anthropic_compat: None,
    }
}

pub fn responses_model(base_url: &str, compat: Option<OpenAIResponsesCompat>) -> Model {
    Model {
        id: "gpt-4o-mini".to_string(),
        name: "GPT-4o mini".to_string(),
        api: "openai-responses".to_string(),
        provider: "openai".to_string(),
        base_url: base_url.to_string(),
        reasoning: false,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 128_000,
        max_tokens: 16_384,
        headers: None,
        openai_completions_compat: None,
        openai_responses_compat: compat,
        anthropic_compat: None,
    }
}

pub fn anthropic_model(base_url: &str, compat: Option<AnthropicMessagesCompat>) -> Model {
    Model {
        id: "claude-haiku-4-5".to_string(),
        name: "Claude Haiku 4.5".to_string(),
        api: "anthropic-messages".to_string(),
        provider: "anthropic".to_string(),
        base_url: base_url.to_string(),
        reasoning: true,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 200_000,
        max_tokens: 8192,
        headers: None,
        openai_completions_compat: None,
        openai_responses_compat: None,
        anthropic_compat: compat,
    }
}

pub fn stream_options_with_cache(cache_retention: CacheRetention, session_id: Option<&str>) -> StreamOptions {
    StreamOptions {
        cache_retention: Some(cache_retention),
        session_id: session_id.map(str::to_string),
        ..Default::default()
    }
}
