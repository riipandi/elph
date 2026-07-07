use std::collections::HashMap;

use anyhow::Result;

use serde_json::{Value, json};

use crate::api::common::{
    apply_on_payload, build_http_client_for_target, finish_stream_error, get_client_api_key,
    invoke_on_response_from_reqwest, is_request_aborted, merge_model_headers,
};
use crate::api::github_copilot_headers::{build_copilot_dynamic_headers, has_copilot_vision_input};
use crate::api::openai_compat::{ResolvedOpenAICompletionsCompat, get_compat, has_tool_history};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::simple_options::build_base_options;
use crate::api::transform_messages::transform_messages;
use crate::models::{calculate_cost, clamp_thinking_level, thinking_level_to_str};
use crate::types::{
    AssistantContentBlock, AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Message, Model,
    ProviderStreams, SimpleStreamOptions, StopReason, StreamOptions, UserContent,
};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::json_parse::parse_streaming_json;
use crate::utils::provider_env::get_provider_env_value;
use crate::utils::sanitize_unicode::sanitize_surrogates;

use super::sse::for_each_sse_json_event;

#[derive(Clone, Default)]
pub struct OpenAICompletionsOptions {
    pub base: StreamOptions,
    pub tool_choice: Option<Value>,
    pub reasoning_effort: Option<String>,
}

pub struct OpenAICompletionsApi;

impl ProviderStreams for OpenAICompletionsApi {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream {
        self.stream_with_options(
            model,
            context,
            OpenAICompletionsOptions {
                base: options.unwrap_or_default(),
                ..Default::default()
            },
        )
    }

    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        let opts = options.as_ref();
        let base = build_base_options(model, context, opts, opts.and_then(|o| o.base.api_key.clone()));
        let reasoning = opts.and_then(|o| o.reasoning).map(|r| clamp_thinking_level(model, r));
        let reasoning_effort = reasoning.map(|r| {
            if r == crate::types::ThinkingLevel::Minimal {
                "minimal".to_string()
            } else {
                thinking_level_to_str(r).to_string()
            }
        });
        self.stream_with_options(
            model,
            context,
            OpenAICompletionsOptions {
                base,
                reasoning_effort,
                ..Default::default()
            },
        )
    }
}

impl OpenAICompletionsApi {
    pub fn stream_with_options(
        &self,
        model: &Model,
        context: &Context,
        options: OpenAICompletionsOptions,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        let model = model.clone();
        let context = context.clone();
        let s = stream.clone();
        tokio::spawn(async move {
            let mut output = AssistantMessage::empty(&model);
            if let Err(e) = run_openai_completions(&model, &context, &options, &s, &mut output).await {
                let aborted = crate::api::common::is_abort_error(&e);
                finish_stream_error(&s, &mut output, e, aborted);
            }
        });
        stream
    }
}

async fn run_openai_completions(
    model: &Model,
    context: &Context,
    options: &OpenAICompletionsOptions,
    stream: &AssistantMessageEventStream,
    output: &mut AssistantMessage,
) -> Result<()> {
    let mut headers = merge_model_headers(model, Some(&options.base));
    if model.provider == "github-copilot" {
        headers.extend(build_copilot_dynamic_headers(
            &context.messages,
            has_copilot_vision_input(&context.messages),
        ));
    }
    let api_key = get_client_api_key(&model.provider, options.base.api_key.as_deref(), &headers)?;
    let mut params = build_params(model, context, options)?;
    params = apply_on_payload(options.base.on_payload.as_ref(), params, model).await;

    let url = format!("{}/chat/completions", model.base_url.trim_end_matches('/'));
    let client = build_http_client_for_target(options.base.timeout_ms, Some(&url), options.base.env.as_ref())?;
    let mut req = client.post(&url).bearer_auth(&api_key).json(&params);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    let response = crate::api::common::send_with_abort(&options.base.signal, req).await?;
    invoke_on_response_from_reqwest(options.base.on_response.as_ref(), &response, model).await;
    let response = crate::api::common::check_response_ok(response).await?;

    stream.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    let mut text_idx: Option<usize> = None;
    let mut thinking_idx: Option<usize> = None;
    let mut tool_blocks: HashMap<usize, (usize, String)> = HashMap::new();
    let mut has_finish = false;

    for_each_sse_json_event(response, &options.base.signal, |chunk| {
        output.response_id = output
            .response_id
            .clone()
            .or_else(|| chunk.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()));
        if let Some(usage) = chunk.get("usage") {
            parse_chunk_usage(output, usage, model);
        }
        let choice = chunk.get("choices").and_then(|c| c.get(0));
        if let Some(choice) = choice {
            if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                output.stop_reason = map_stop_reason(reason);
                has_finish = true;
            }
            if let Some(delta) = choice.get("delta") {
                if let Some(text) = delta.get("content").and_then(|v| v.as_str())
                    && !text.is_empty()
                {
                    let idx = ensure_text_block(output, stream, &mut text_idx);
                    if let AssistantContentBlock::Text(t) = &mut output.content[idx] {
                        t.text.push_str(text);
                        stream.push(AssistantMessageEvent::TextDelta {
                            content_index: idx,
                            delta: text.to_string(),
                            partial: output.clone(),
                        });
                    }
                }
                let mut found_reasoning_field: Option<&str> = None;
                for field in ["reasoning_content", "reasoning", "reasoning_text"] {
                    if let Some(reasoning) = delta.get(field).and_then(|v| v.as_str())
                        && !reasoning.is_empty()
                    {
                        found_reasoning_field = Some(field);
                        break;
                    }
                }
                if let Some(field) = found_reasoning_field
                    && let Some(reasoning) = delta.get(field).and_then(|v| v.as_str())
                {
                    let signature = if model.provider == "opencode-go" && field == "reasoning" {
                        "reasoning_content"
                    } else {
                        field
                    };
                    let idx = ensure_thinking_block(output, stream, &mut thinking_idx, signature);
                    if let AssistantContentBlock::Thinking(t) = &mut output.content[idx] {
                        t.thinking.push_str(reasoning);
                        stream.push(AssistantMessageEvent::ThinkingDelta {
                            content_index: idx,
                            delta: reasoning.to_string(),
                            partial: output.clone(),
                        });
                    }
                }
                if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tool_calls {
                        let stream_index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let (idx, partial) = tool_blocks.entry(stream_index).or_insert_with(|| {
                            let idx = output.content.len();
                            output
                                .content
                                .push(AssistantContentBlock::ToolCall(crate::types::ToolCall::new(
                                    tc.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                                    tc.pointer("/function/name").and_then(|v| v.as_str()).unwrap_or(""),
                                    json!({}),
                                )));
                            stream.push(AssistantMessageEvent::ToolcallStart {
                                content_index: idx,
                                partial: output.clone(),
                            });
                            (idx, String::new())
                        });
                        if let Some(args) = tc.pointer("/function/arguments").and_then(|v| v.as_str()) {
                            partial.push_str(args);
                            if let AssistantContentBlock::ToolCall(tool) = &mut output.content[*idx] {
                                tool.arguments = parse_streaming_json(Some(partial));
                                stream.push(AssistantMessageEvent::ToolcallDelta {
                                    content_index: *idx,
                                    delta: args.to_string(),
                                    partial: output.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    })
    .await?;

    if let Some(idx) = text_idx
        && let AssistantContentBlock::Text(t) = &output.content[idx]
    {
        stream.push(AssistantMessageEvent::TextEnd {
            content_index: idx,
            content: t.text.clone(),
            partial: output.clone(),
        });
    }
    if let Some(idx) = thinking_idx
        && let AssistantContentBlock::Thinking(t) = &output.content[idx]
    {
        stream.push(AssistantMessageEvent::ThinkingEnd {
            content_index: idx,
            content: t.thinking.clone(),
            partial: output.clone(),
        });
    }
    for (_, (idx, partial)) in tool_blocks {
        if let AssistantContentBlock::ToolCall(tc) = &mut output.content[idx] {
            tc.arguments = parse_streaming_json(Some(&partial));
            stream.push(AssistantMessageEvent::ToolcallEnd {
                content_index: idx,
                tool_call: tc.clone(),
                partial: output.clone(),
            });
        }
    }
    if is_request_aborted(&options.base.signal) {
        output.stop_reason = StopReason::Aborted;
    } else if !has_finish {
        return Err(anyhow::anyhow!("Stream ended without finish_reason"));
    }
    stream.push(AssistantMessageEvent::Done {
        reason: output.stop_reason,
        message: output.clone(),
    });
    stream.end();
    Ok(())
}

fn ensure_text_block(
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    idx: &mut Option<usize>,
) -> usize {
    if let Some(i) = *idx {
        return i;
    }
    let i = output.content.len();
    output
        .content
        .push(AssistantContentBlock::Text(crate::types::TextContent::new("")));
    stream.push(AssistantMessageEvent::TextStart {
        content_index: i,
        partial: output.clone(),
    });
    *idx = Some(i);
    i
}

fn ensure_thinking_block(
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    idx: &mut Option<usize>,
    sig: &str,
) -> usize {
    if let Some(i) = *idx {
        return i;
    }
    let mut t = crate::types::ThinkingContent::new("");
    t.thinking_signature = Some(sig.to_string());
    let i = output.content.len();
    output.content.push(AssistantContentBlock::Thinking(t));
    stream.push(AssistantMessageEvent::ThinkingStart {
        content_index: i,
        partial: output.clone(),
    });
    *idx = Some(i);
    i
}

#[derive(Clone, Debug)]
struct OpenAICompatCacheControl {
    kind: String,
    ttl: Option<String>,
}

impl OpenAICompatCacheControl {
    fn to_value(&self) -> Value {
        let mut obj = json!({ "type": self.kind });
        if let Some(ttl) = &self.ttl {
            obj["ttl"] = json!(ttl);
        }
        obj
    }
}

fn get_compat_cache_control(
    compat: &ResolvedOpenAICompletionsCompat,
    cache_retention: crate::types::CacheRetention,
) -> Option<OpenAICompatCacheControl> {
    if compat.cache_control_format.as_deref() != Some("anthropic")
        || cache_retention == crate::types::CacheRetention::None
    {
        return None;
    }
    let ttl = if cache_retention == crate::types::CacheRetention::Long && compat.supports_long_cache_retention {
        Some("1h".to_string())
    } else {
        None
    };
    Some(OpenAICompatCacheControl {
        kind: "ephemeral".to_string(),
        ttl,
    })
}

fn add_cache_control_to_text_content(message: &mut Value, cache_control: &OpenAICompatCacheControl) -> bool {
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
            "cache_control": cache_control.to_value(),
        }]);
        return true;
    }
    let Some(parts) = content.as_array_mut() else {
        return false;
    };
    for part in parts.iter_mut().rev() {
        if part.get("type").and_then(|v| v.as_str()) == Some("text") {
            part["cache_control"] = cache_control.to_value();
            return true;
        }
    }
    false
}

fn apply_anthropic_cache_control(
    messages: &mut [Value],
    tools: Option<&mut Value>,
    cache_control: &OpenAICompatCacheControl,
) {
    for message in messages.iter_mut() {
        let role = message.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if role == "system" || role == "developer" {
            add_cache_control_to_text_content(message, cache_control);
            break;
        }
    }
    if let Some(tools) = tools
        && let Some(last) = tools.as_array_mut().and_then(|arr| arr.last_mut())
    {
        last["cache_control"] = cache_control.to_value();
    }
    for message in messages.iter_mut().rev() {
        let role = message.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if (role == "user" || role == "assistant") && add_cache_control_to_text_content(message, cache_control) {
            break;
        }
    }
}

fn convert_tools(tools: &[crate::types::Tool], compat: &ResolvedOpenAICompletionsCompat) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            let mut function = json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            });
            if compat.supports_strict_mode {
                function["strict"] = json!(false);
            }
            json!({ "type": "function", "function": function })
        })
        .collect()
}

/// Build chat/completions request params (used by tests mirroring elph-ai payload capture).
pub fn build_openai_completions_params(
    model: &Model,
    context: &Context,
    options: &OpenAICompletionsOptions,
) -> Result<Value> {
    build_params(model, context, options)
}

fn build_params(model: &Model, context: &Context, options: &OpenAICompletionsOptions) -> Result<Value> {
    let compat = get_compat(model);
    let cache_retention = resolve_cache_retention(&options.base);
    let mut messages = convert_messages(model, context, &compat);
    let cache_control = get_compat_cache_control(&compat, cache_retention);
    let mut params = json!({
        "model": model.id,
        "messages": messages,
        "stream": true,
    });
    if compat.supports_usage_in_streaming {
        params["stream_options"] = json!({ "include_usage": true });
    }
    if compat.supports_store {
        params["store"] = json!(false);
    }
    if let Some(max) = options.base.max_tokens {
        params[&compat.max_tokens_field] = json!(max);
    }
    if let Some(temp) = options.base.temperature {
        params["temperature"] = json!(temp);
    }
    let mut tools_value = if let Some(tools) = &context.tools {
        if !tools.is_empty() {
            Some(json!(convert_tools(tools, &compat)))
        } else if has_tool_history(&context.messages) {
            Some(json!([]))
        } else {
            None
        }
    } else if has_tool_history(&context.messages) {
        Some(json!([]))
    } else {
        None
    };
    if let Some(ref tools) = tools_value {
        params["tools"] = tools.clone();
        if compat.zai_tool_stream {
            params["tool_stream"] = json!(true);
        }
    }
    if let Some(cache_control) = &cache_control {
        apply_anthropic_cache_control(&mut messages, tools_value.as_mut(), cache_control);
        params["messages"] = json!(messages);
        if let Some(tools) = tools_value {
            params["tools"] = tools;
        }
    }
    apply_thinking_params(model, options, &compat, &mut params);
    if cache_retention != crate::types::CacheRetention::None {
        let use_cache = model.base_url.contains("api.openai.com")
            || (cache_retention == crate::types::CacheRetention::Long && compat.supports_long_cache_retention);
        if use_cache {
            if let Some(key) = clamp_openai_prompt_cache_key(options.base.session_id.as_deref()) {
                params["prompt_cache_key"] = json!(key);
            }
            if cache_retention == crate::types::CacheRetention::Long && compat.supports_long_cache_retention {
                params["prompt_cache_retention"] = json!("24h");
            }
        }
    }
    if let Some(choice) = &options.tool_choice {
        params["tool_choice"] = choice.clone();
    }
    Ok(params)
}

fn apply_thinking_params(
    model: &Model,
    options: &OpenAICompletionsOptions,
    compat: &crate::api::openai_compat::ResolvedOpenAICompletionsCompat,
    params: &mut Value,
) {
    let effort = options.reasoning_effort.as_deref();
    if !model.reasoning {
        return;
    }
    match compat.thinking_format.as_str() {
        "zai" => {
            params["thinking"] = if effort.is_some() {
                json!({ "type": "enabled", "clear_thinking": false })
            } else {
                json!({ "type": "disabled" })
            };
            if let Some(effort) = effort
                && compat.supports_reasoning_effort
            {
                let mapped = thinking_level_value(model, effort).unwrap_or_else(|| effort.to_string());
                params["reasoning_effort"] = json!(mapped);
            }
        }
        "qwen" => {
            params["enable_thinking"] = json!(effort.is_some());
        }
        "qwen-chat-template" => {
            params["chat_template_kwargs"] = json!({
                "enable_thinking": effort.is_some(),
                "preserve_thinking": true
            });
        }
        "deepseek" => {
            if effort.is_some() {
                params["thinking"] = json!({ "type": "enabled" });
            } else {
                params["thinking"] = json!({ "type": "disabled" });
            }
            if let Some(effort) = effort
                && compat.supports_reasoning_effort
            {
                let mapped = thinking_level_value(model, effort).unwrap_or_else(|| effort.to_string());
                params["reasoning_effort"] = json!(mapped);
            }
        }
        "openrouter" => {
            if let Some(effort) = effort {
                let mapped = thinking_level_value(model, effort).unwrap_or_else(|| effort.to_string());
                params["reasoning"] = json!({ "effort": mapped });
            } else {
                let off = thinking_level_value(model, "off").unwrap_or_else(|| "none".to_string());
                params["reasoning"] = json!({ "effort": off });
            }
        }
        "ant-ling" => {
            if let Some(effort) = effort
                && let Some(mapped) = thinking_level_value(model, effort)
            {
                params["reasoning"] = json!({ "effort": mapped });
            }
        }
        "together" => {
            params["reasoning"] = json!({ "enabled": effort.is_some() });
            if let Some(effort) = effort
                && compat.supports_reasoning_effort
            {
                let mapped = thinking_level_value(model, effort).unwrap_or_else(|| effort.to_string());
                params["reasoning_effort"] = json!(mapped);
            }
        }
        "string-thinking" => {
            if let Some(effort) = effort {
                let mapped = thinking_level_value(model, effort).unwrap_or_else(|| effort.to_string());
                params["thinking"] = json!(mapped);
            } else {
                let off = thinking_level_value(model, "off").unwrap_or_else(|| "none".to_string());
                params["thinking"] = json!(off);
            }
        }
        _ => {
            if let Some(effort) = effort {
                if compat.supports_reasoning_effort {
                    let mapped = thinking_level_value(model, effort).unwrap_or_else(|| effort.to_string());
                    params["reasoning_effort"] = json!(mapped);
                }
            } else if compat.supports_reasoning_effort
                && let Some(off) = thinking_level_value(model, "off")
            {
                params["reasoning_effort"] = json!(off);
            }
        }
    }
}

pub fn convert_messages(model: &Model, context: &Context, compat: &ResolvedOpenAICompletionsCompat) -> Vec<Value> {
    let transformed = transform_messages(context.messages.clone(), model, |id, _, _| {
        if id.contains('|') {
            let call_id = id.split('|').next().unwrap_or(id);
            return call_id
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .take(40)
                .collect();
        }
        if model.provider == "openai" && id.len() > 40 {
            return id.chars().take(40).collect();
        }
        id.to_string()
    });

    let mut params = Vec::new();
    if let Some(sp) = &context.system_prompt {
        let role = if model.reasoning && compat.supports_developer_role {
            "developer"
        } else {
            "system"
        };
        params.push(json!({ "role": role, "content": sanitize_surrogates(sp) }));
    }

    let mut last_role: Option<&str> = None;
    let mut i = 0usize;
    while i < transformed.len() {
        let msg = &transformed[i];
        if compat.requires_assistant_after_tool_result
            && last_role == Some("toolResult")
            && matches!(msg, Message::User { .. })
        {
            params.push(json!({
                "role": "assistant",
                "content": "I have processed the tool results."
            }));
        }

        match msg {
            Message::User { content, .. } => match content {
                UserContent::Text(t) => {
                    params.push(json!({ "role": "user", "content": sanitize_surrogates(t) }));
                    last_role = Some("user");
                }
                UserContent::Blocks(blocks) => {
                    let content: Vec<Value> = blocks
                        .iter()
                        .map(|b| match b {
                            ContentBlock::Text { text } => json!({
                                "type": "text",
                                "text": sanitize_surrogates(text)
                            }),
                            ContentBlock::Image { data, mime_type } => json!({
                                "type": "image_url",
                                "image_url": { "url": format!("data:{mime_type};base64,{data}") }
                            }),
                        })
                        .collect();
                    if !content.is_empty() {
                        params.push(json!({ "role": "user", "content": content }));
                        last_role = Some("user");
                    }
                }
            },
            Message::Assistant(a) => {
                let assistant_text_parts: Vec<Value> = a
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        AssistantContentBlock::Text(t) if !t.text.trim().is_empty() => Some(json!({
                            "type": "text",
                            "text": sanitize_surrogates(&t.text)
                        })),
                        _ => None,
                    })
                    .collect();
                let assistant_text = assistant_text_parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join("");

                let thinking_blocks: Vec<&crate::types::ThinkingContent> = a
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        AssistantContentBlock::Thinking(t) if !t.thinking.trim().is_empty() => Some(t),
                        _ => None,
                    })
                    .collect();

                let mut assistant_msg = json!({
                    "role": "assistant",
                    "content": if compat.requires_assistant_after_tool_result {
                        Value::String(String::new())
                    } else {
                        Value::Null
                    }
                });

                if !thinking_blocks.is_empty() {
                    if compat.requires_thinking_as_text {
                        let thinking_text = thinking_blocks
                            .iter()
                            .map(|b| sanitize_surrogates(&b.thinking))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        let mut parts = vec![json!({ "type": "text", "text": thinking_text })];
                        parts.extend(assistant_text_parts);
                        assistant_msg["content"] = json!(parts);
                    } else {
                        if !assistant_text.is_empty() {
                            assistant_msg["content"] = json!(assistant_text);
                        }
                        let mut signature = thinking_blocks[0].thinking_signature.clone().unwrap_or_default();
                        if model.provider == "opencode-go" && signature == "reasoning" {
                            signature = "reasoning_content".to_string();
                        }
                        if !signature.is_empty() {
                            let thinking_joined = thinking_blocks
                                .iter()
                                .map(|b| b.thinking.as_str())
                                .collect::<Vec<_>>()
                                .join("\n");
                            assistant_msg[&signature] = json!(thinking_joined);
                        }
                    }
                } else if !assistant_text.is_empty() {
                    assistant_msg["content"] = json!(assistant_text);
                }

                let tool_calls: Vec<Value> = a
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        AssistantContentBlock::ToolCall(tc) => Some(json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments.to_string()
                            }
                        })),
                        _ => None,
                    })
                    .collect();
                if !tool_calls.is_empty() {
                    assistant_msg["tool_calls"] = json!(tool_calls);
                    let reasoning_details: Vec<Value> = a
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            AssistantContentBlock::ToolCall(tc) => tc
                                .thought_signature
                                .as_ref()
                                .and_then(|sig| serde_json::from_str::<Value>(sig).ok()),
                            _ => None,
                        })
                        .collect();
                    if !reasoning_details.is_empty() {
                        assistant_msg["reasoning_details"] = json!(reasoning_details);
                    }
                }

                if compat.requires_reasoning_content_on_assistant_messages
                    && model.reasoning
                    && assistant_msg.get("reasoning_content").is_none()
                {
                    assistant_msg["reasoning_content"] = json!("");
                }

                let has_content = match assistant_msg.get("content") {
                    Some(Value::Null) => false,
                    Some(Value::String(s)) => !s.is_empty(),
                    Some(Value::Array(arr)) => !arr.is_empty(),
                    _ => false,
                };
                if has_content || assistant_msg.get("tool_calls").is_some() {
                    params.push(assistant_msg);
                    last_role = Some("assistant");
                }
            }
            Message::ToolResult { .. } => {
                let mut image_blocks = Vec::new();
                let mut j = i;
                while j < transformed.len() {
                    let Message::ToolResult {
                        tool_call_id: tc_id,
                        tool_name: tn,
                        content: blocks,
                        ..
                    } = &transformed[j]
                    else {
                        break;
                    };

                    let text_result = blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let has_images = blocks.iter().any(|b| matches!(b, ContentBlock::Image { .. }));
                    let has_text = !text_result.is_empty();
                    let tool_result_text = if has_text {
                        text_result
                    } else if has_images {
                        "(see attached image)".to_string()
                    } else {
                        "(no tool output)".to_string()
                    };

                    let mut tool_result_msg = json!({
                        "role": "tool",
                        "tool_call_id": tc_id,
                        "content": sanitize_surrogates(&tool_result_text),
                    });
                    if compat.requires_tool_result_name && !tn.is_empty() {
                        tool_result_msg["name"] = json!(tn);
                    }
                    params.push(tool_result_msg);

                    if has_images && model.input.iter().any(|i| i == "image") {
                        for block in blocks {
                            if let ContentBlock::Image { data, mime_type } = block {
                                image_blocks.push(json!({
                                    "type": "image_url",
                                    "image_url": { "url": format!("data:{mime_type};base64,{data}") }
                                }));
                            }
                        }
                    }
                    j += 1;
                }
                i = j.saturating_sub(1);

                if !image_blocks.is_empty() {
                    if compat.requires_assistant_after_tool_result {
                        params.push(json!({
                            "role": "assistant",
                            "content": "I have processed the tool results."
                        }));
                    }
                    let mut user_content = vec![json!({
                        "type": "text",
                        "text": "Attached image(s) from tool result:"
                    })];
                    user_content.extend(image_blocks);
                    params.push(json!({ "role": "user", "content": user_content }));
                    last_role = Some("user");
                } else {
                    last_role = Some("toolResult");
                }
            }
        }
        i += 1;
    }
    params
}

fn parse_chunk_usage(output: &mut AssistantMessage, usage: &Value, model: &Model) {
    let prompt = usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_read = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_write = usage
        .pointer("/prompt_tokens_details/cache_write_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    output.usage.input = prompt.saturating_sub(cache_read).saturating_sub(cache_write);
    output.usage.output = usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    output.usage.cache_read = cache_read;
    output.usage.cache_write = cache_write;
    output.usage.reasoning = usage
        .pointer("/completion_tokens_details/reasoning_tokens")
        .and_then(|v| v.as_u64());
    output.usage.total_tokens =
        output.usage.input + output.usage.output + output.usage.cache_read + output.usage.cache_write;
    calculate_cost(model, &mut output.usage);
}

fn map_stop_reason(reason: &str) -> StopReason {
    match reason {
        "stop" | "end" => StopReason::Stop,
        "length" => StopReason::Length,
        "function_call" | "tool_calls" => StopReason::ToolUse,
        "content_filter" | "network_error" => StopReason::Error,
        _ => StopReason::Error,
    }
}

fn thinking_level_value(model: &Model, level: &str) -> Option<String> {
    model
        .thinking_level_map
        .as_ref()
        .and_then(|m| m.get(level))
        .and_then(|v| v.clone())
}

fn resolve_cache_retention(options: &StreamOptions) -> crate::types::CacheRetention {
    if let Some(r) = options.cache_retention {
        return r;
    }
    if get_provider_env_value("PI_CACHE_RETENTION", options.env.as_ref()) == Some("long".to_string()) {
        crate::types::CacheRetention::Long
    } else {
        crate::types::CacheRetention::Short
    }
}
