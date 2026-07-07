use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::types::{
    AssistantContentBlock, AssistantMessage, AssistantMessageEvent, Context, Model, ProviderStreams,
    SimpleStreamOptions, StopReason, StreamOptions, TextContent, ThinkingContent, ToolCall,
};
use crate::utils::event_stream::AssistantMessageEventStream;

const DEFAULT_API: &str = "faux";
const DEFAULT_PROVIDER: &str = "faux";
const DEFAULT_MODEL_ID: &str = "faux-1";
const DEFAULT_BASE_URL: &str = "http://localhost:0";
const DEFAULT_MIN_TOKEN_SIZE: usize = 3;
const DEFAULT_MAX_TOKEN_SIZE: usize = 5;

pub type FauxResponseFactory =
    Arc<dyn Fn(&Context, Option<&StreamOptions>, &FauxState, &Model) -> AssistantMessage + Send + Sync>;

#[allow(clippy::large_enum_variant)]
pub enum FauxResponseStep {
    Static(AssistantMessage),
    Factory(FauxResponseFactory),
}

#[derive(Debug, Default, Clone)]
pub struct FauxState {
    pub call_count: u64,
}

#[derive(Default)]
pub struct RegisterFauxProviderOptions {
    pub api: Option<String>,
    pub provider: Option<String>,
    pub models: Option<Vec<FauxModelDefinition>>,
    pub tokens_per_second: Option<f64>,
    pub token_size_min: Option<usize>,
    pub token_size_max: Option<usize>,
}

#[derive(Clone)]
pub struct FauxModelDefinition {
    pub id: String,
    pub name: Option<String>,
    pub reasoning: Option<bool>,
    pub input: Option<Vec<String>>,
    pub context_window: Option<u32>,
    pub max_tokens: Option<u32>,
}

pub struct FauxCore {
    pub api: String,
    pub provider: String,
    pub models: Vec<Model>,
    pub state: Arc<Mutex<FauxState>>,
    pending: Arc<Mutex<Vec<FauxResponseStep>>>,
    tokens_per_second: Option<f64>,
    min_token_size: usize,
    max_token_size: usize,
    prompt_cache: Arc<Mutex<HashMap<String, String>>>,
}

pub struct FauxApi {
    core: Arc<FauxCore>,
}

impl FauxCore {
    pub fn new(options: RegisterFauxProviderOptions) -> Self {
        let min = options.token_size_min.unwrap_or(DEFAULT_MIN_TOKEN_SIZE).max(1);
        let max = options.token_size_max.unwrap_or(DEFAULT_MAX_TOKEN_SIZE).max(min);
        let api = options.api.unwrap_or_else(|| DEFAULT_API.to_string());
        let provider = options.provider.unwrap_or_else(|| DEFAULT_PROVIDER.to_string());
        let defs = options.models.unwrap_or_else(|| {
            vec![FauxModelDefinition {
                id: DEFAULT_MODEL_ID.to_string(),
                name: Some("Faux Model".to_string()),
                reasoning: Some(false),
                input: Some(vec!["text".to_string(), "image".to_string()]),
                context_window: Some(128_000),
                max_tokens: Some(16_384),
            }]
        });
        let models = defs
            .into_iter()
            .map(|d| Model {
                id: d.id.clone(),
                name: d.name.unwrap_or_else(|| d.id.clone()),
                api: api.clone(),
                provider: provider.clone(),
                base_url: DEFAULT_BASE_URL.to_string(),
                reasoning: d.reasoning.unwrap_or(false),
                thinking_level_map: None,
                input: d.input.unwrap_or_else(|| vec!["text".to_string(), "image".to_string()]),
                cost: crate::types::ModelCost {
                    input: 0.0,
                    output: 0.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                },
                context_window: d.context_window.unwrap_or(128_000),
                max_tokens: d.max_tokens.unwrap_or(16_384),
                headers: None,
                openai_completions_compat: None,
                openai_responses_compat: None,
                anthropic_compat: None,
            })
            .collect();

        Self {
            api,
            provider,
            models,
            state: Arc::new(Mutex::new(FauxState::default())),
            pending: Arc::new(Mutex::new(Vec::new())),
            tokens_per_second: options.tokens_per_second,
            min_token_size: min,
            max_token_size: max,
            prompt_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_responses(&self, responses: Vec<FauxResponseStep>) {
        *self.pending.lock().unwrap() = responses;
    }

    pub fn append_responses(&self, responses: Vec<FauxResponseStep>) {
        self.pending.lock().unwrap().extend(responses);
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    pub fn api(self: &Arc<Self>) -> FauxApi {
        FauxApi { core: Arc::clone(self) }
    }
}

impl ProviderStreams for FauxApi {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream {
        self.stream_simple(model, context, options.map(SimpleStreamOptions::from_stream))
    }

    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        let core = self.core.clone();
        let model = model.clone();
        let context = context.clone();
        let options = options.map(|o| o.base);
        let s = stream.clone();
        tokio::spawn(async move {
            if let Err(e) = run_faux(&core, &model, &context, options.as_ref(), &s).await {
                let mut output = AssistantMessage::empty(&model);
                output.stop_reason = StopReason::Error;
                output.error_message = Some(e.to_string());
                s.push(AssistantMessageEvent::Error {
                    reason: StopReason::Error,
                    error: output.clone(),
                });
                s.end();
            }
        });
        stream
    }
}

async fn run_faux(
    core: &FauxCore,
    model: &Model,
    context: &Context,
    options: Option<&StreamOptions>,
    stream: &AssistantMessageEventStream,
) -> anyhow::Result<()> {
    if crate::api::common::is_request_aborted(&options.and_then(|o| o.signal.clone())) {
        let mut output = AssistantMessage::empty(model);
        crate::api::common::finish_stream_error(stream, &mut output, crate::api::common::request_aborted_error(), true);
        return Ok(());
    }
    {
        let mut state = core.state.lock().unwrap();
        state.call_count += 1;
    }

    let step = core.pending.lock().unwrap().drain(..1).next();
    let state = core.state.lock().unwrap().clone();

    let message = match step {
        Some(FauxResponseStep::Static(m)) => m,
        Some(FauxResponseStep::Factory(f)) => f(context, options, &state, model),
        None => {
            let mut m = AssistantMessage::empty(model);
            m.stop_reason = StopReason::Error;
            m.error_message = Some("No more faux responses queued".to_string());
            m
        }
    };

    let message = with_usage_estimate(message, context, options, &core.prompt_cache);
    stream_with_deltas(
        stream,
        message,
        core.min_token_size,
        core.max_token_size,
        core.tokens_per_second,
        options.and_then(|o| o.signal.clone()),
    )
    .await;
    Ok(())
}

pub fn faux_text(text: impl Into<String>) -> AssistantContentBlock {
    AssistantContentBlock::Text(TextContent::new(text))
}

pub fn faux_thinking(thinking: impl Into<String>) -> AssistantContentBlock {
    AssistantContentBlock::Thinking(ThinkingContent::new(thinking))
}

pub fn faux_tool_call(name: impl Into<String>, arguments: Value, id: Option<String>) -> AssistantContentBlock {
    AssistantContentBlock::ToolCall(ToolCall::new(
        id.unwrap_or_else(|| format!("tool:{}", chrono::Utc::now().timestamp_millis())),
        name,
        arguments,
    ))
}

pub fn faux_assistant_message(
    content: Vec<AssistantContentBlock>,
    stop_reason: Option<StopReason>,
) -> AssistantMessage {
    let mut m = AssistantMessage::empty(&Model {
        id: DEFAULT_MODEL_ID.to_string(),
        name: DEFAULT_MODEL_ID.to_string(),
        api: DEFAULT_API.to_string(),
        provider: DEFAULT_PROVIDER.to_string(),
        base_url: DEFAULT_BASE_URL.to_string(),
        reasoning: false,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: crate::types::ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 128_000,
        max_tokens: 16_384,
        headers: None,
        openai_completions_compat: None,
        openai_responses_compat: None,
        anthropic_compat: None,
    });
    m.content = content;
    m.stop_reason = stop_reason.unwrap_or(StopReason::Stop);
    m
}

fn estimate_tokens(text: &str) -> u64 {
    ((text.len() as f64) / 4.0).ceil() as u64
}

fn with_usage_estimate(
    mut message: AssistantMessage,
    context: &Context,
    options: Option<&StreamOptions>,
    prompt_cache: &Arc<Mutex<HashMap<String, String>>>,
) -> AssistantMessage {
    let prompt_text = serialize_context(context);
    let prompt_tokens = estimate_tokens(&prompt_text);
    let output_text: String = message
        .content
        .iter()
        .map(|b| match b {
            AssistantContentBlock::Text(t) => t.text.clone(),
            AssistantContentBlock::Thinking(t) => t.thinking.clone(),
            AssistantContentBlock::ToolCall(tc) => format!("{}:{}", tc.name, tc.arguments),
        })
        .collect::<Vec<_>>()
        .join("");
    let output_tokens = estimate_tokens(&output_text);
    let mut input = prompt_tokens;
    let mut cache_read = 0u64;
    let mut cache_write = 0u64;

    if let Some(session_id) = options.and_then(|o| o.session_id.as_deref()) {
        let mut cache = prompt_cache.lock().unwrap();
        if let Some(previous) = cache.get(session_id).cloned() {
            let cached_chars = common_prefix_len(&previous, &prompt_text);
            cache_read = estimate_tokens(&previous[..cached_chars.min(previous.len())]);
            cache_write = estimate_tokens(&prompt_text[cached_chars.min(prompt_text.len())..]);
            input = prompt_tokens.saturating_sub(cache_read);
        } else {
            cache_write = prompt_tokens;
        }
        cache.insert(session_id.to_string(), prompt_text);
    }

    message.usage.input = input;
    message.usage.output = output_tokens;
    message.usage.cache_read = cache_read;
    message.usage.cache_write = cache_write;
    message.usage.total_tokens = input + output_tokens + cache_read + cache_write;
    message
}

fn serialize_context(context: &Context) -> String {
    let mut parts = Vec::new();
    if let Some(sp) = &context.system_prompt {
        parts.push(format!("system:{sp}"));
    }
    for msg in &context.messages {
        parts.push(format!("{msg:?}"));
    }
    if let Some(tools) = &context.tools {
        parts.push(format!("tools:{}", serde_json::to_string(tools).unwrap_or_default()));
    }
    parts.join("\n\n")
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

fn split_by_token_size(text: &str, min: usize, max: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut index = 0;
    let bytes = text.as_bytes();
    while index < bytes.len() {
        let token_size = {
            use rand::RngExt;
            rand::rng().random_range(min..=max)
        };
        let char_size = (token_size * 4).max(1);
        let end = (index + char_size).min(bytes.len());
        chunks.push(String::from_utf8_lossy(&bytes[index..end]).to_string());
        index = end;
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }
    chunks
}

async fn schedule_chunk(chunk: &str, tokens_per_second: Option<f64>) {
    if let Some(tps) = tokens_per_second
        && tps > 0.0
    {
        let delay_ms = (estimate_tokens(chunk) as f64 / tps * 1000.0) as u64;
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
    }
}

async fn stream_with_deltas(
    stream: &AssistantMessageEventStream,
    message: AssistantMessage,
    min_token_size: usize,
    max_token_size: usize,
    tokens_per_second: Option<f64>,
    signal: Option<tokio_util::sync::CancellationToken>,
) {
    let mut partial = AssistantMessage {
        content: vec![],
        ..message.clone()
    };
    stream.push(AssistantMessageEvent::Start {
        partial: partial.clone(),
    });

    for (index, block) in message.content.iter().enumerate() {
        if crate::api::common::is_request_aborted(&signal) {
            let mut output = partial.clone();
            output.stop_reason = StopReason::Aborted;
            crate::api::common::finish_stream_error(
                stream,
                &mut output,
                crate::api::common::request_aborted_error(),
                true,
            );
            return;
        }
        match block {
            AssistantContentBlock::Thinking(t) => {
                partial
                    .content
                    .push(AssistantContentBlock::Thinking(ThinkingContent::new("")));
                stream.push(AssistantMessageEvent::ThinkingStart {
                    content_index: index,
                    partial: partial.clone(),
                });
                for chunk in split_by_token_size(&t.thinking, min_token_size, max_token_size) {
                    schedule_chunk(&chunk, tokens_per_second).await;
                    if crate::api::common::is_request_aborted(&signal) {
                        let mut output = partial.clone();
                        output.stop_reason = StopReason::Aborted;
                        crate::api::common::finish_stream_error(
                            stream,
                            &mut output,
                            crate::api::common::request_aborted_error(),
                            true,
                        );
                        return;
                    }
                    if let AssistantContentBlock::Thinking(tc) = &mut partial.content[index] {
                        tc.thinking.push_str(&chunk);
                    }
                    stream.push(AssistantMessageEvent::ThinkingDelta {
                        content_index: index,
                        delta: chunk,
                        partial: partial.clone(),
                    });
                }
                stream.push(AssistantMessageEvent::ThinkingEnd {
                    content_index: index,
                    content: t.thinking.clone(),
                    partial: partial.clone(),
                });
            }
            AssistantContentBlock::Text(t) => {
                partial.content.push(AssistantContentBlock::Text(TextContent::new("")));
                stream.push(AssistantMessageEvent::TextStart {
                    content_index: index,
                    partial: partial.clone(),
                });
                for chunk in split_by_token_size(&t.text, min_token_size, max_token_size) {
                    schedule_chunk(&chunk, tokens_per_second).await;
                    if crate::api::common::is_request_aborted(&signal) {
                        let mut output = partial.clone();
                        output.stop_reason = StopReason::Aborted;
                        crate::api::common::finish_stream_error(
                            stream,
                            &mut output,
                            crate::api::common::request_aborted_error(),
                            true,
                        );
                        return;
                    }
                    if let AssistantContentBlock::Text(tc) = &mut partial.content[index] {
                        tc.text.push_str(&chunk);
                    }
                    stream.push(AssistantMessageEvent::TextDelta {
                        content_index: index,
                        delta: chunk,
                        partial: partial.clone(),
                    });
                }
                stream.push(AssistantMessageEvent::TextEnd {
                    content_index: index,
                    content: t.text.clone(),
                    partial: partial.clone(),
                });
            }
            AssistantContentBlock::ToolCall(tc) => {
                partial.content.push(AssistantContentBlock::ToolCall(ToolCall::new(
                    &tc.id,
                    &tc.name,
                    Value::Object(Default::default()),
                )));
                stream.push(AssistantMessageEvent::ToolcallStart {
                    content_index: index,
                    partial: partial.clone(),
                });
                let args = tc.arguments.to_string();
                for chunk in split_by_token_size(&args, min_token_size, max_token_size) {
                    schedule_chunk(&chunk, tokens_per_second).await;
                    if crate::api::common::is_request_aborted(&signal) {
                        let mut output = partial.clone();
                        output.stop_reason = StopReason::Aborted;
                        crate::api::common::finish_stream_error(
                            stream,
                            &mut output,
                            crate::api::common::request_aborted_error(),
                            true,
                        );
                        return;
                    }
                    stream.push(AssistantMessageEvent::ToolcallDelta {
                        content_index: index,
                        delta: chunk,
                        partial: partial.clone(),
                    });
                }
                if let AssistantContentBlock::ToolCall(slot) = &mut partial.content[index] {
                    slot.arguments = tc.arguments.clone();
                }
                stream.push(AssistantMessageEvent::ToolcallEnd {
                    content_index: index,
                    tool_call: tc.clone(),
                    partial: partial.clone(),
                });
            }
        }
    }

    if matches!(message.stop_reason, StopReason::Error | StopReason::Aborted) {
        stream.push(AssistantMessageEvent::Error {
            reason: message.stop_reason,
            error: message.clone(),
        });
        stream.end();
        return;
    }

    stream.push(AssistantMessageEvent::Done {
        reason: message.stop_reason,
        message: message.clone(),
    });
    stream.end();
}
