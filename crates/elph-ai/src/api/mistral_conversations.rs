use anyhow::{Result, anyhow};

use serde_json::{Value, json};

use crate::api::common::{
    apply_on_payload, build_http_client, finish_stream_error, invoke_on_response_from_reqwest, merge_model_headers,
};
use crate::api::simple_options::build_base_options;
use crate::api::transform_messages::transform_messages;
use crate::models::{calculate_cost, clamp_thinking_level, thinking_level_to_str};
use crate::types::{
    AssistantContentBlock, AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Message, Model,
    ProviderStreams, SimpleStreamOptions, StopReason, StreamOptions, UserContent,
};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::hash::short_hash;
use crate::utils::json_parse::parse_streaming_json;
use crate::utils::sanitize_unicode::sanitize_surrogates;

use super::sse::collect_sse_json_events;

const MISTRAL_TOOL_CALL_ID_LENGTH: usize = 9;

#[derive(Clone, Default)]
pub struct MistralOptions {
    pub base: StreamOptions,
    pub tool_choice: Option<Value>,
    pub prompt_mode: Option<String>,
    pub reasoning_effort: Option<String>,
}

pub struct MistralConversationsApi;

impl ProviderStreams for MistralConversationsApi {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream {
        self.stream_with_options(
            model,
            context,
            MistralOptions {
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
        let should_reason = model.reasoning && reasoning.is_some();
        self.stream_with_options(
            model,
            context,
            MistralOptions {
                base,
                prompt_mode: if should_reason && uses_prompt_mode_reasoning(model) {
                    Some("reasoning".to_string())
                } else {
                    None
                },
                reasoning_effort: if should_reason && uses_reasoning_effort(model) {
                    reasoning.map(|r| {
                        model
                            .thinking_level_map
                            .as_ref()
                            .and_then(|m| m.get(thinking_level_to_str(r)).cloned().flatten())
                            .unwrap_or_else(|| "high".to_string())
                    })
                } else {
                    None
                },
                ..Default::default()
            },
        )
    }
}

impl MistralConversationsApi {
    pub fn stream_with_options(
        &self,
        model: &Model,
        context: &Context,
        options: MistralOptions,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        let model = model.clone();
        let context = context.clone();
        let s = stream.clone();
        tokio::spawn(async move {
            let mut output = AssistantMessage::empty(&model);
            if let Err(e) = run_mistral(&model, &context, &options, &s, &mut output).await {
                finish_stream_error(&s, &mut output, e, false);
            }
        });
        stream
    }
}

async fn run_mistral(
    model: &Model,
    context: &Context,
    options: &MistralOptions,
    stream: &AssistantMessageEventStream,
    output: &mut AssistantMessage,
) -> Result<()> {
    let api_key = options
        .base
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow!("No API key for provider: {}", model.provider))?;
    let normalize = create_mistral_tool_call_id_normalizer();
    let transformed = transform_messages(context.messages.clone(), model, |id, _, _| normalize(id));
    let mut payload = build_chat_payload(model, context, &transformed, options)?;
    payload = apply_on_payload(options.base.on_payload.as_ref(), payload, model).await;
    let headers = merge_model_headers(model, Some(&options.base));

    let client = build_http_client(options.base.timeout_ms)?;
    let url = format!("{}/v1/chat/completions", model.base_url.trim_end_matches('/'));
    let mut req = client.post(&url).bearer_auth(api_key).json(&payload);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    let response = req.send().await?;
    invoke_on_response_from_reqwest(options.base.on_response.as_ref(), &response, model).await;
    let response = crate::api::common::check_response_ok(response).await?;

    stream.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    let mut current_block: Option<usize> = None;
    let mut tool_blocks: std::collections::HashMap<String, (usize, String)> = std::collections::HashMap::new();
    let chunks = collect_sse_json_events(response).await?;
    for chunk in chunks {
        output.response_id = output
            .response_id
            .clone()
            .or_else(|| chunk.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()));
        if let Some(usage) = chunk.get("usage") {
            let prompt = usage
                .get("prompt_tokens")
                .or_else(|| usage.get("promptTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            output.usage.input = prompt;
            output.usage.output = usage
                .get("completion_tokens")
                .or_else(|| usage.get("completionTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            output.usage.total_tokens = usage
                .get("total_tokens")
                .or_else(|| usage.get("totalTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            calculate_cost(model, &mut output.usage);
        }
        let choice = chunk.get("choices").and_then(|c| c.get(0));
        if let Some(choice) = choice {
            if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                output.stop_reason = map_chat_stop_reason(reason);
            }
            if let Some(delta) = choice.get("delta") {
                if let Some(content) = delta.get("content") {
                    process_content_delta(content, output, stream, &mut current_block);
                }
                if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tool_calls {
                        let call_id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("null");
                        let key = format!("{call_id}:{}", tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0));
                        let (idx, partial) = tool_blocks.entry(key).or_insert_with(|| {
                            let idx = output.content.len();
                            output
                                .content
                                .push(AssistantContentBlock::ToolCall(crate::types::ToolCall::new(
                                    call_id,
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
    }
    finish_current(output, stream, &mut current_block);
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
    stream.push(AssistantMessageEvent::Done {
        reason: output.stop_reason,
        message: output.clone(),
    });
    stream.end();
    Ok(())
}

fn process_content_delta(
    content: &Value,
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    current: &mut Option<usize>,
) {
    let items = if let Some(s) = content.as_str() {
        vec![json!(s)]
    } else if let Some(arr) = content.as_array() {
        arr.clone()
    } else {
        return;
    };
    for item in items {
        if let Some(s) = item.as_str() {
            let delta = sanitize_surrogates(s);
            let idx = ensure_text(output, stream, current);
            if let AssistantContentBlock::Text(t) = &mut output.content[idx] {
                t.text.push_str(&delta);
                stream.push(AssistantMessageEvent::TextDelta {
                    content_index: idx,
                    delta,
                    partial: output.clone(),
                });
            }
        } else if item.get("type").and_then(|v| v.as_str()) == Some("thinking") {
            let delta = item
                .get("thinking")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            let delta = sanitize_surrogates(&delta);
            if delta.is_empty() {
                continue;
            }
            let idx = ensure_thinking(output, stream, current);
            if let AssistantContentBlock::Thinking(t) = &mut output.content[idx] {
                t.thinking.push_str(&delta);
                stream.push(AssistantMessageEvent::ThinkingDelta {
                    content_index: idx,
                    delta,
                    partial: output.clone(),
                });
            }
        }
    }
}

fn ensure_text(
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    current: &mut Option<usize>,
) -> usize {
    if matches!(
        current.and_then(|i| output.content.get(i)),
        Some(AssistantContentBlock::Text(_))
    ) {
        return current.unwrap();
    }
    finish_current(output, stream, current);
    let idx = output.content.len();
    output
        .content
        .push(AssistantContentBlock::Text(crate::types::TextContent::new("")));
    stream.push(AssistantMessageEvent::TextStart {
        content_index: idx,
        partial: output.clone(),
    });
    *current = Some(idx);
    idx
}

fn ensure_thinking(
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    current: &mut Option<usize>,
) -> usize {
    if matches!(
        current.and_then(|i| output.content.get(i)),
        Some(AssistantContentBlock::Thinking(_))
    ) {
        return current.unwrap();
    }
    finish_current(output, stream, current);
    let idx = output.content.len();
    output
        .content
        .push(AssistantContentBlock::Thinking(crate::types::ThinkingContent::new("")));
    stream.push(AssistantMessageEvent::ThinkingStart {
        content_index: idx,
        partial: output.clone(),
    });
    *current = Some(idx);
    idx
}

fn finish_current(output: &mut AssistantMessage, stream: &AssistantMessageEventStream, current: &mut Option<usize>) {
    if let Some(idx) = current.take() {
        match &output.content[idx] {
            AssistantContentBlock::Text(t) => stream.push(AssistantMessageEvent::TextEnd {
                content_index: idx,
                content: t.text.clone(),
                partial: output.clone(),
            }),
            AssistantContentBlock::Thinking(t) => stream.push(AssistantMessageEvent::ThinkingEnd {
                content_index: idx,
                content: t.thinking.clone(),
                partial: output.clone(),
            }),
            _ => {}
        }
    }
}

/// Build Mistral chat payload (used by integration tests mirroring pi-ai).
pub fn build_mistral_conversations_payload(
    model: &Model,
    context: &Context,
    messages: &[Message],
    options: &MistralOptions,
) -> Result<Value> {
    build_chat_payload(model, context, messages, options)
}

fn build_chat_payload(
    model: &Model,
    context: &Context,
    messages: &[Message],
    options: &MistralOptions,
) -> Result<Value> {
    let mut payload = json!({
        "model": model.id,
        "stream": true,
        "messages": to_chat_messages(messages, model.input.iter().any(|i| i == "image"))
    });
    if let Some(sp) = &context.system_prompt {
        payload["messages"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({ "role": "system", "content": sanitize_surrogates(sp) }));
    }
    if let Some(tools) = &context.tools
        && !tools.is_empty()
    {
        payload["tools"] = json!(tools.iter().map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.parameters, "strict": false } })).collect::<Vec<_>>());
    }
    if let Some(temp) = options.base.temperature {
        payload["temperature"] = json!(temp);
    }
    if let Some(max) = options.base.max_tokens {
        payload["max_tokens"] = json!(max);
    }
    if let Some(mode) = &options.prompt_mode {
        payload["prompt_mode"] = json!(mode);
    }
    if let Some(effort) = &options.reasoning_effort {
        payload["reasoning_effort"] = json!(effort);
    }
    if options.base.cache_retention != Some(crate::types::CacheRetention::None)
        && let Some(sid) = &options.base.session_id
    {
        payload["prompt_cache_key"] = json!(sid);
    }
    Ok(payload)
}

fn to_chat_messages(messages: &[Message], supports_images: bool) -> Vec<Value> {
    messages.iter().filter_map(|msg| match msg {
        Message::User { content, .. } => match content {
            UserContent::Text(t) => Some(json!({ "role": "user", "content": sanitize_surrogates(t) })),
            UserContent::Blocks(blocks) => {
                let content: Vec<Value> = blocks.iter().filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(json!({ "type": "text", "text": sanitize_surrogates(text) })),
                    ContentBlock::Image { data, mime_type } if supports_images => Some(json!({ "type": "image_url", "image_url": format!("data:{mime_type};base64,{data}") })),
                    _ => None,
                }).collect();
                if content.is_empty() { None } else { Some(json!({ "role": "user", "content": content })) }
            }
        },
        Message::Assistant(a) => {
            let text: Vec<Value> = a.content.iter().filter_map(|b| match b {
                AssistantContentBlock::Text(t) if !t.text.trim().is_empty() => Some(json!({ "type": "text", "text": sanitize_surrogates(&t.text) })),
                _ => None,
            }).collect();
            let tool_calls: Vec<Value> = a.content.iter().filter_map(|b| match b {
                AssistantContentBlock::ToolCall(tc) => Some(json!({ "id": tc.id, "type": "function", "function": { "name": tc.name, "arguments": tc.arguments.to_string() } })),
                _ => None,
            }).collect();
            let mut m = json!({ "role": "assistant" });
            if !text.is_empty() { m["content"] = json!(text); }
            if !tool_calls.is_empty() { m["tool_calls"] = json!(tool_calls); }
            if m.get("content").is_some() || m.get("tool_calls").is_some() { Some(m) } else { None }
        }
        Message::ToolResult { tool_call_id, tool_name, content, is_error, .. } => {
            let text = content.iter().filter_map(|b| match b { ContentBlock::Text { text } => Some(text.as_str()), _ => None }).collect::<Vec<_>>().join("\n");
            Some(json!({ "role": "tool", "tool_call_id": tool_call_id, "name": tool_name, "content": [{ "type": "text", "text": if *is_error { format!("[tool error] {text}") } else { text } }] }))
        }
    }).collect()
}

fn create_mistral_tool_call_id_normalizer() -> impl Fn(&str) -> String {
    move |id: &str| {
        let normalized: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if normalized.len() == MISTRAL_TOOL_CALL_ID_LENGTH {
            normalized
        } else {
            short_hash(&normalized)
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .take(MISTRAL_TOOL_CALL_ID_LENGTH)
                .collect()
        }
    }
}

fn uses_reasoning_effort(model: &Model) -> bool {
    matches!(
        model.id.as_str(),
        "mistral-small-2603" | "mistral-small-latest" | "mistral-medium-3.5"
    )
}

fn uses_prompt_mode_reasoning(model: &Model) -> bool {
    model.reasoning && !uses_reasoning_effort(model)
}

fn map_chat_stop_reason(reason: &str) -> StopReason {
    match reason {
        "stop" => StopReason::Stop,
        "length" | "model_length" => StopReason::Length,
        "tool_calls" => StopReason::ToolUse,
        "error" => StopReason::Error,
        _ => StopReason::Stop,
    }
}
