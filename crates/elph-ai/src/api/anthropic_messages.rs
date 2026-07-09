use anyhow::{Result, anyhow};

use serde_json::{Value, json};

use crate::api::common::{
    apply_on_payload, build_http_client_for_target, finish_stream_error, get_client_api_key,
    invoke_on_response_from_reqwest, is_request_aborted, merge_model_headers,
};
use crate::api::github_copilot_headers::{build_copilot_dynamic_headers, has_copilot_vision_input};
use crate::api::simple_options::{adjust_max_tokens_for_thinking, build_base_options, clamp_max_tokens_to_context};
use crate::api::sse::{ANTHROPIC_MESSAGE_EVENTS, ServerSentEvent, decode_sse_buffer, for_each_anthropic_sse_event};
use crate::api::transform_messages::transform_messages;
use crate::models::{calculate_cost, thinking_level_to_str};
use crate::types::{
    AssistantContentBlock, AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Message, Model,
    ProviderStreams, SimpleStreamOptions, StopReason, StreamOptions, ThinkingLevel, UserContent,
};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::json_parse::{parse_json_with_repair, parse_streaming_json};
use crate::utils::provider_env::get_provider_env_value;
use crate::utils::sanitize_unicode::sanitize_surrogates;

#[derive(Clone, Default)]
pub struct AnthropicOptions {
    pub base: StreamOptions,
    pub thinking_enabled: Option<bool>,
    pub thinking_budget_tokens: Option<u32>,
    pub effort: Option<String>,
    pub thinking_display: Option<String>,
    pub interleaved_thinking: Option<bool>,
    pub tool_choice: Option<Value>,
}

pub struct AnthropicMessagesApi;

impl ProviderStreams for AnthropicMessagesApi {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream {
        let anthropic_opts = AnthropicOptions {
            base: options.unwrap_or_default(),
            ..Default::default()
        };
        self.stream_with_options(model, context, anthropic_opts)
    }

    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        let opts = options.as_ref();
        let base = build_base_options(model, context, opts, opts.and_then(|o| o.base.api_key.clone()));
        if opts.and_then(|o| o.reasoning).is_none() {
            return self.stream_with_options(
                model,
                context,
                AnthropicOptions {
                    base,
                    thinking_enabled: Some(false),
                    ..Default::default()
                },
            );
        }
        let reasoning = opts.unwrap().reasoning.unwrap();
        if model.anthropic_compat.as_ref().and_then(|c| c.force_adaptive_thinking) == Some(true) {
            return self.stream_with_options(
                model,
                context,
                AnthropicOptions {
                    base,
                    thinking_enabled: Some(true),
                    effort: Some(map_thinking_level_to_effort(model, reasoning)),
                    ..Default::default()
                },
            );
        }
        let (max_tokens, thinking_budget) = adjust_max_tokens_for_thinking(
            base.max_tokens,
            model.max_tokens,
            reasoning,
            opts.and_then(|o| o.thinking_budgets.as_ref()),
        );
        let max_tokens = clamp_max_tokens_to_context(model, context, max_tokens);
        self.stream_with_options(
            model,
            context,
            AnthropicOptions {
                base: StreamOptions {
                    max_tokens: Some(max_tokens),
                    ..base
                },
                thinking_enabled: Some(true),
                thinking_budget_tokens: Some(thinking_budget.min(max_tokens.saturating_sub(1024))),
                ..Default::default()
            },
        )
    }
}

impl AnthropicMessagesApi {
    pub fn stream_with_options(
        &self,
        model: &Model,
        context: &Context,
        options: AnthropicOptions,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        let model = model.clone();
        let context = context.clone();
        let stream_clone = stream.clone();

        tokio::spawn(async move {
            let mut output = AssistantMessage::empty(&model);
            if let Err(error) = run_anthropic_stream(&model, &context, &options, &stream_clone, &mut output).await {
                let aborted = crate::api::common::is_abort_error(&error);
                finish_stream_error(&stream_clone, &mut output, error, aborted);
            }
        });

        stream
    }
}

async fn run_anthropic_stream(
    model: &Model,
    context: &Context,
    options: &AnthropicOptions,
    stream: &AssistantMessageEventStream,
    output: &mut AssistantMessage,
) -> Result<()> {
    let api_key = options.base.api_key.as_deref();
    let mut headers = merge_model_headers(model, Some(&options.base));
    if model.provider == "github-copilot" {
        headers.extend(build_copilot_dynamic_headers(
            &context.messages,
            has_copilot_vision_input(&context.messages),
        ));
    }
    get_client_api_key(&model.provider, api_key, &headers)?;

    let mut params = build_params(model, context, options)?;
    params = apply_on_payload(options.base.on_payload.as_ref(), params, model).await;

    let url = format!("{}/v1/messages", model.base_url.trim_end_matches('/'));
    let client = build_http_client_for_target(options.base.timeout_ms, Some(&url), options.base.env.as_ref())?;
    let mut req = client.post(&url).json(&params);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    if let Some(key) = api_key {
        if model.provider != "github-copilot" && !key.contains("sk-ant-oat") {
            req = req.header("x-api-key", key);
            req = req.header("anthropic-version", "2023-06-01");
        } else {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
    }

    if crate::api::common::is_request_aborted(&options.base.signal) {
        crate::api::common::finish_stream_error(stream, output, crate::api::common::request_aborted_error(), true);
        return Ok(());
    }

    let response = crate::api::common::send_with_abort(&options.base.signal, req).await?;
    invoke_on_response_from_reqwest(options.base.on_response.as_ref(), &response, model).await;
    let response = crate::api::common::check_response_ok(response).await?;

    stream.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });

    let mut state = AnthropicStreamState::default();
    for_each_anthropic_sse_event(response, &options.base.signal, |sse| {
        process_anthropic_sse_event(&sse, &mut state, output, stream, model)
    })
    .await?;

    if is_request_aborted(&options.base.signal) {
        output.stop_reason = StopReason::Aborted;
    } else if state.saw_start && !state.saw_end {
        return Err(anyhow!("Anthropic stream ended before message_stop"));
    }
    stream.push(AssistantMessageEvent::Done {
        reason: output.stop_reason,
        message: output.clone(),
    });
    stream.end();
    Ok(())
}

#[derive(Default)]
struct AnthropicStreamState {
    saw_start: bool,
    saw_end: bool,
    blocks: Vec<(usize, AssistantContentBlock, Option<String>)>,
}

fn process_anthropic_sse_event(
    sse: &ServerSentEvent,
    state: &mut AnthropicStreamState,
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    model: &Model,
) -> Result<()> {
    if sse.event.as_deref() == Some("error") {
        return Err(anyhow!(sse.data.clone()));
    }
    if !ANTHROPIC_MESSAGE_EVENTS.contains(&sse.event.as_deref().unwrap_or("")) {
        return Ok(());
    }
    let event: Value = parse_json_with_repair(&sse.data)?;
    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match event_type {
        "message_start" => {
            state.saw_start = true;
            if let Some(id) = event.pointer("/message/id").and_then(|v| v.as_str()) {
                output.response_id = Some(id.to_string());
            }
            if let Some(usage) = event.pointer("/message/usage") {
                update_usage_from_anthropic(output, usage);
                calculate_cost(model, &mut output.usage);
            }
        }
        "content_block_start" => {
            let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let block_type = event
                .pointer("/content_block/type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let block = match block_type {
                "text" => AssistantContentBlock::Text(crate::types::TextContent::new("")),
                "thinking" => AssistantContentBlock::Thinking(crate::types::ThinkingContent::new("")),
                "tool_use" => AssistantContentBlock::ToolCall(crate::types::ToolCall::new(
                    event
                        .pointer("/content_block/id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    event
                        .pointer("/content_block/name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    event.pointer("/content_block/input").cloned().unwrap_or(json!({})),
                )),
                _ => return Ok(()),
            };
            let content_index = output.content.len();
            output.content.push(block.clone());
            state.blocks.push((index, block, None));
            match output.content.last().unwrap() {
                AssistantContentBlock::Text(_) => stream.push(AssistantMessageEvent::TextStart {
                    content_index,
                    partial: output.clone(),
                }),
                AssistantContentBlock::Thinking(_) => stream.push(AssistantMessageEvent::ThinkingStart {
                    content_index,
                    partial: output.clone(),
                }),
                AssistantContentBlock::ToolCall(_) => stream.push(AssistantMessageEvent::ToolcallStart {
                    content_index,
                    partial: output.clone(),
                }),
            }
        }
        "content_block_delta" => {
            let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let delta_type = event.pointer("/delta/type").and_then(|v| v.as_str()).unwrap_or("");
            let pos = state.blocks.iter().position(|(i, _, _)| *i == index);
            if let Some(pos) = pos {
                let content_index = pos;
                match delta_type {
                    "text_delta" => {
                        let delta = event.pointer("/delta/text").and_then(|v| v.as_str()).unwrap_or("");
                        if let AssistantContentBlock::Text(t) = &mut output.content[content_index] {
                            t.text.push_str(delta);
                            stream.push(AssistantMessageEvent::TextDelta {
                                content_index,
                                delta: delta.to_string(),
                                partial: output.clone(),
                            });
                        }
                    }
                    "thinking_delta" => {
                        let delta = event.pointer("/delta/thinking").and_then(|v| v.as_str()).unwrap_or("");
                        if let AssistantContentBlock::Thinking(t) = &mut output.content[content_index] {
                            t.thinking.push_str(delta);
                            stream.push(AssistantMessageEvent::ThinkingDelta {
                                content_index,
                                delta: delta.to_string(),
                                partial: output.clone(),
                            });
                        }
                    }
                    "input_json_delta" => {
                        let delta = event
                            .pointer("/delta/partial_json")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        state.blocks[pos].2 = Some(state.blocks[pos].2.clone().unwrap_or_default() + delta);
                        if let AssistantContentBlock::ToolCall(tc) = &mut output.content[content_index] {
                            let partial = state.blocks[pos].2.as_deref().unwrap_or("");
                            tc.arguments = parse_streaming_json(Some(partial));
                            stream.push(AssistantMessageEvent::ToolcallDelta {
                                content_index,
                                delta: delta.to_string(),
                                partial: output.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        "content_block_stop" => {
            let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if let Some(pos) = state.blocks.iter().position(|(i, _, _)| *i == index) {
                let content_index = pos;
                match &output.content[content_index] {
                    AssistantContentBlock::Text(t) => stream.push(AssistantMessageEvent::TextEnd {
                        content_index,
                        content: t.text.clone(),
                        partial: output.clone(),
                    }),
                    AssistantContentBlock::Thinking(t) => stream.push(AssistantMessageEvent::ThinkingEnd {
                        content_index,
                        content: t.thinking.clone(),
                        partial: output.clone(),
                    }),
                    AssistantContentBlock::ToolCall(tc) => stream.push(AssistantMessageEvent::ToolcallEnd {
                        content_index,
                        tool_call: tc.clone(),
                        partial: output.clone(),
                    }),
                }
            }
        }
        "message_delta" => {
            if let Some(reason) = event.pointer("/delta/stop_reason").and_then(|v| v.as_str()) {
                let stop_details = event.pointer("/delta/stop_details");
                let result = map_stop_reason(reason, stop_details);
                output.stop_reason = result.stop_reason;
                if let Some(message) = result.error_message {
                    output.error_message = Some(message);
                }
            }
            if let Some(usage) = event.get("usage") {
                update_usage_from_anthropic(output, usage);
                calculate_cost(model, &mut output.usage);
            }
        }
        "message_stop" => state.saw_end = true,
        _ => {}
    }
    Ok(())
}

/// Process raw Anthropic Messages SSE bytes (used by integration tests mirroring elph-ai).
pub async fn process_anthropic_sse_buffer(
    buffer: &str,
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    model: &Model,
) -> Result<()> {
    let mut decoder = crate::api::sse::SseDecoderState::default();
    let mut state = AnthropicStreamState::default();
    for sse in decode_sse_buffer(buffer, &mut decoder) {
        process_anthropic_sse_event(&sse, &mut state, output, stream, model)?;
    }

    if state.saw_start && !state.saw_end {
        return Err(anyhow!("Anthropic stream ended before message_stop"));
    }
    stream.push(AssistantMessageEvent::Done {
        reason: output.stop_reason,
        message: output.clone(),
    });
    stream.end();
    Ok(())
}

fn update_usage_from_anthropic(output: &mut AssistantMessage, usage: &Value) {
    if let Some(v) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
        output.usage.input = v;
    }
    if let Some(v) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
        output.usage.output = v;
    }
    if let Some(v) = usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()) {
        output.usage.cache_read = v;
    }
    if let Some(v) = usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()) {
        output.usage.cache_write = v;
    }
    output.usage.total_tokens =
        output.usage.input + output.usage.output + output.usage.cache_read + output.usage.cache_write;
}

/// Build Anthropic Messages request params (used by integration tests mirroring elph-ai).
pub fn build_anthropic_messages_params(model: &Model, context: &Context, options: &AnthropicOptions) -> Result<Value> {
    build_params(model, context, options)
}

struct ResolvedAnthropicCompat {
    supports_long_cache_retention: bool,
    supports_cache_control_on_tools: bool,
    supports_eager_tool_input_streaming: bool,
    supports_temperature: bool,
}

fn get_anthropic_compat(model: &Model) -> ResolvedAnthropicCompat {
    let compat = model.anthropic_compat.as_ref();
    ResolvedAnthropicCompat {
        supports_long_cache_retention: compat.and_then(|c| c.supports_long_cache_retention).unwrap_or(true),
        supports_cache_control_on_tools: compat.and_then(|c| c.supports_cache_control_on_tools).unwrap_or(true),
        supports_eager_tool_input_streaming: compat
            .and_then(|c| c.supports_eager_tool_input_streaming)
            .unwrap_or(true),
        supports_temperature: compat.and_then(|c| c.supports_temperature).unwrap_or(true),
    }
}

fn anthropic_cache_control(model: &Model, retention: crate::types::CacheRetention) -> Option<Value> {
    if retention == crate::types::CacheRetention::None {
        return None;
    }
    let compat = get_anthropic_compat(model);
    let mut control = json!({ "type": "ephemeral" });
    if retention == crate::types::CacheRetention::Long && compat.supports_long_cache_retention {
        control["ttl"] = json!("1h");
    }
    Some(control)
}

fn add_cache_control_to_text_content(message: &mut Value, cache_control: &Value) -> bool {
    let Some(content) = message.get_mut("content") else {
        return false;
    };
    if let Some(text) = content.as_str() {
        if text.is_empty() {
            return false;
        }
        *content = json!([{
            "type": "text",
            "text": text,
            "cache_control": cache_control.clone(),
        }]);
        return true;
    }
    let Some(parts) = content.as_array_mut() else {
        return false;
    };
    for part in parts.iter_mut().rev() {
        if part.get("type").and_then(|v| v.as_str()) == Some("text") {
            part["cache_control"] = cache_control.clone();
            return true;
        }
    }
    false
}

fn apply_anthropic_payload_cache_control(params: &mut Value, model: &Model, retention: crate::types::CacheRetention) {
    let Some(cache_control) = anthropic_cache_control(model, retention) else {
        return;
    };
    let compat = get_anthropic_compat(model);
    if let Some(system) = params.get_mut("system").and_then(|v| v.as_array_mut())
        && let Some(first) = system.first_mut()
    {
        first["cache_control"] = cache_control.clone();
    }
    if compat.supports_cache_control_on_tools
        && let Some(tools) = params.get_mut("tools").and_then(|v| v.as_array_mut())
        && let Some(last) = tools.last_mut()
    {
        last["cache_control"] = cache_control.clone();
    }
    if let Some(messages) = params.get_mut("messages").and_then(|v| v.as_array_mut()) {
        for message in messages.iter_mut().rev() {
            let role = message.get("role").and_then(|v| v.as_str()).unwrap_or("");
            if (role == "user" || role == "assistant") && add_cache_control_to_text_content(message, &cache_control) {
                break;
            }
        }
    }
}

fn build_params(model: &Model, context: &Context, options: &AnthropicOptions) -> Result<Value> {
    let messages = convert_messages(context, model);
    let mut params = json!({
        "model": model.id,
        "messages": messages,
        "max_tokens": options.base.max_tokens.unwrap_or(model.max_tokens),
        "stream": true
    });
    if let Some(sp) = &context.system_prompt {
        params["system"] = json!([{ "type": "text", "text": sanitize_surrogates(sp) }]);
    }
    let compat = get_anthropic_compat(model);
    if compat.supports_temperature
        && options.thinking_enabled != Some(true)
        && let Some(temp) = options.base.temperature
    {
        params["temperature"] = json!(temp);
    }
    if let Some(tools) = &context.tools
        && !tools.is_empty()
    {
        let eager = compat.supports_eager_tool_input_streaming;
        params["tools"] = json!(tools.iter().map(|t| {
                let mut tool = json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": { "type": "object", "properties": t.parameters.get("properties").cloned().unwrap_or(json!({})), "required": t.parameters.get("required").cloned().unwrap_or(json!([])) }
                });
                if eager {
                    tool["eager_input_streaming"] = json!(true);
                }
                tool
            }).collect::<Vec<_>>());
    }
    if options.thinking_enabled == Some(true) {
        if model.anthropic_compat.as_ref().and_then(|c| c.force_adaptive_thinking) == Some(true) {
            let mut thinking = json!({ "type": "adaptive" });
            if let Some(effort) = &options.effort {
                thinking["effort"] = json!(effort);
            }
            params["thinking"] = thinking;
        } else {
            params["thinking"] =
                json!({ "type": "enabled", "budget_tokens": options.thinking_budget_tokens.unwrap_or(1024) });
        }
    }
    let cache_retention = resolve_cache_retention(&options.base);
    apply_anthropic_payload_cache_control(&mut params, model, cache_retention);
    Ok(params)
}

fn convert_messages(context: &Context, model: &Model) -> Vec<Value> {
    let transformed = transform_messages(context.messages.clone(), model, |id, _, _| {
        id.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .take(64)
            .collect()
    });
    transformed
        .into_iter()
        .filter_map(|msg| match msg {
            Message::User { content, .. } => {
                let content = match content {
                    UserContent::Text(t) => json!(sanitize_surrogates(&t)),
                    UserContent::Blocks(blocks) => json!(blocks.into_iter().map(|b| match b {
                        ContentBlock::Text { text } => json!({ "type": "text", "text": sanitize_surrogates(&text) }),
                        ContentBlock::Image { data, mime_type } => json!({ "type": "image", "source": { "type": "base64", "media_type": mime_type, "data": data } }),
                    }).collect::<Vec<_>>()),
                };
                Some(json!({ "role": "user", "content": content }))
            }
            Message::Assistant(a) => {
                let blocks: Vec<Value> = a.content.iter().filter_map(|b| match b {
                    AssistantContentBlock::Text(t) if !t.text.trim().is_empty() => Some(json!({ "type": "text", "text": sanitize_surrogates(&t.text) })),
                    AssistantContentBlock::Thinking(t) if !t.thinking.trim().is_empty() => {
                        if t.redacted == Some(true) {
                            Some(json!({ "type": "redacted_thinking", "data": t.thinking_signature.clone().unwrap_or_default() }))
                        } else {
                            let signature = t.thinking_signature.as_deref().unwrap_or("");
                            let allow_empty = model
                                .anthropic_compat
                                .as_ref()
                                .and_then(|c| c.allow_empty_signature)
                                .unwrap_or(false);
                            if signature.trim().is_empty() {
                                if allow_empty {
                                    Some(json!({ "type": "thinking", "thinking": sanitize_surrogates(&t.thinking), "signature": "" }))
                                } else {
                                    Some(json!({ "type": "text", "text": sanitize_surrogates(&t.thinking) }))
                                }
                            } else {
                                Some(json!({ "type": "thinking", "thinking": sanitize_surrogates(&t.thinking), "signature": signature }))
                            }
                        }
                    }
                    AssistantContentBlock::ToolCall(tc) => Some(json!({ "type": "tool_use", "id": tc.id, "name": tc.name, "input": tc.arguments })),
                    _ => None,
                }).collect();
                if blocks.is_empty() { None } else { Some(json!({ "role": "assistant", "content": blocks })) }
            }
            Message::ToolResult { tool_call_id, content, is_error, .. } => Some(json!({
                "role": "user",
                "content": [{ "type": "tool_result", "tool_use_id": tool_call_id, "content": sanitize_surrogates(&content.iter().filter_map(|b| match b { ContentBlock::Text { text } => Some(text.as_str()), _ => None }).collect::<Vec<_>>().join("\n")), "is_error": is_error }]
            })),
        })
        .collect()
}

struct AnthropicStopReasonResult {
    stop_reason: StopReason,
    error_message: Option<String>,
}

fn map_stop_reason(reason: &str, stop_details: Option<&Value>) -> AnthropicStopReasonResult {
    match reason {
        "end_turn" | "pause_turn" | "stop_sequence" => AnthropicStopReasonResult {
            stop_reason: StopReason::Stop,
            error_message: None,
        },
        "max_tokens" => AnthropicStopReasonResult {
            stop_reason: StopReason::Length,
            error_message: None,
        },
        "tool_use" => AnthropicStopReasonResult {
            stop_reason: StopReason::ToolUse,
            error_message: None,
        },
        "refusal" => AnthropicStopReasonResult {
            stop_reason: StopReason::Error,
            error_message: Some(
                stop_details
                    .and_then(|d| d.get("explanation"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| "The model refused to complete the request".to_string()),
            ),
        },
        _ => AnthropicStopReasonResult {
            stop_reason: StopReason::Error,
            error_message: None,
        },
    }
}

fn map_thinking_level_to_effort(model: &Model, level: ThinkingLevel) -> String {
    if let Some(map) = &model.thinking_level_map
        && let Some(Some(v)) = map.get(thinking_level_to_str(level))
    {
        return v.clone();
    }
    match level {
        ThinkingLevel::Minimal | ThinkingLevel::Low => "low".to_string(),
        ThinkingLevel::Medium => "medium".to_string(),
        ThinkingLevel::High | ThinkingLevel::Xhigh => "high".to_string(),
    }
}

fn resolve_cache_retention(options: &StreamOptions) -> crate::types::CacheRetention {
    if let Some(r) = options.cache_retention {
        return r;
    }
    if get_provider_env_value("ELPH_CACHE_RETENTION", options.env.as_ref()) == Some("long".to_string()) {
        return crate::types::CacheRetention::Long;
    }
    crate::types::CacheRetention::Short
}
