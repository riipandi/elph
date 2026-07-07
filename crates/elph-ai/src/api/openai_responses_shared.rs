use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

use crate::models::calculate_cost;
use crate::types::{
    AssistantContentBlock, AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Message, Model, StopReason,
    TextContent, TextSignatureV1, ThinkingContent, ToolCall, Usage, UserContent,
};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::hash::short_hash;
use crate::utils::json_parse::parse_streaming_json;
use crate::utils::sanitize_unicode::sanitize_surrogates;

use super::transform_messages::transform_messages;

fn encode_text_signature_v1(id: &str, phase: Option<&str>) -> String {
    let mut payload = serde_json::json!({ "v": 1, "id": id });
    if let Some(phase) = phase {
        payload["phase"] = json!(phase);
    }
    payload.to_string()
}

fn parse_text_signature(signature: Option<&str>) -> Option<(String, Option<String>)> {
    let sig = signature?;
    if sig.starts_with('{')
        && let Ok(parsed) = serde_json::from_str::<TextSignatureV1>(sig)
        && parsed.v == 1
    {
        return Some((parsed.id, parsed.phase));
    }
    Some((sig.to_string(), None))
}

type ServiceTierResolver = Box<dyn Fn(Option<&str>, Option<&str>) -> Option<String> + Send + Sync>;
type ServiceTierPricingApplier = Box<dyn Fn(&mut Usage, Option<&str>) + Send + Sync>;

pub struct OpenAIResponsesStreamOptions {
    pub service_tier: Option<String>,
    pub resolve_service_tier: Option<ServiceTierResolver>,
    pub apply_service_tier_pricing: Option<ServiceTierPricingApplier>,
}

pub struct ConvertResponsesMessagesOptions {
    pub include_system_prompt: bool,
}

pub fn convert_responses_messages(
    model: &Model,
    context: &Context,
    allowed_tool_call_providers: &HashSet<String>,
    options: Option<ConvertResponsesMessagesOptions>,
) -> Vec<Value> {
    let include_system = options.map(|o| o.include_system_prompt).unwrap_or(true);
    let mut messages = Vec::new();

    let normalize_id_part = |part: &str| -> String {
        let sanitized: String = part
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let normalized: String = sanitized.chars().take(64).collect();
        normalized.trim_end_matches('_').to_string()
    };

    let normalize_tool_call_id = |id: &str, source: &AssistantMessage| -> String {
        if !allowed_tool_call_providers.contains(&model.provider) {
            return normalize_id_part(id);
        }
        if !id.contains('|') {
            return normalize_id_part(id);
        }
        let parts: Vec<&str> = id.splitn(2, '|').collect();
        let call_id = normalize_id_part(parts[0]);
        let item_id = parts.get(1).copied().unwrap_or("");
        let is_foreign = source.provider != model.provider || source.api != model.api;
        let mut normalized_item_id = if is_foreign {
            format!("fc_{}", short_hash(item_id))
        } else {
            normalize_id_part(item_id)
        };
        if normalized_item_id.len() > 64 {
            normalized_item_id = normalized_item_id.chars().take(64).collect();
        }
        if !normalized_item_id.starts_with("fc_") {
            normalized_item_id = normalize_id_part(&format!("fc_{normalized_item_id}"));
        }
        format!("{call_id}|{normalized_item_id}")
    };

    let transformed = transform_messages(context.messages.clone(), model, |id, _m, src| {
        normalize_tool_call_id(id, src)
    });

    if include_system && let Some(sp) = &context.system_prompt {
        let role = if model.reasoning { "developer" } else { "system" };
        messages.push(json!({ "role": role, "content": sanitize_surrogates(sp) }));
    }

    for (msg_index, msg) in transformed.into_iter().enumerate() {
        match msg {
            Message::User { content, .. } => match content {
                UserContent::Text(text) => {
                    messages.push(json!({
                        "role": "user",
                        "content": [{ "type": "input_text", "text": sanitize_surrogates(&text) }]
                    }));
                }
                UserContent::Blocks(blocks) => {
                    let content: Vec<Value> = blocks
                        .into_iter()
                        .map(|b| match b {
                            ContentBlock::Text { text } => {
                                json!({ "type": "input_text", "text": sanitize_surrogates(&text) })
                            }
                            ContentBlock::Image { data, mime_type } => json!({
                                "type": "input_image",
                                "detail": "auto",
                                "image_url": format!("data:{mime_type};base64,{data}")
                            }),
                        })
                        .collect();
                    if !content.is_empty() {
                        messages.push(json!({ "role": "user", "content": content }));
                    }
                }
            },
            Message::Assistant(assistant) => {
                let is_different_model =
                    assistant.model != model.id && assistant.provider == model.provider && assistant.api == model.api;
                let mut output = Vec::new();
                let mut text_block_index = 0usize;
                for block in &assistant.content {
                    match block {
                        AssistantContentBlock::Thinking(t) => {
                            if let Some(sig) = &t.thinking_signature
                                && let Ok(item) = serde_json::from_str::<Value>(sig)
                            {
                                output.push(item);
                            }
                        }
                        AssistantContentBlock::Text(text) => {
                            let parsed = parse_text_signature(text.text_signature.as_deref());
                            let fallback = if text_block_index == 0 {
                                format!("msg_pi_{msg_index}")
                            } else {
                                format!("msg_pi_{msg_index}_{text_block_index}")
                            };
                            text_block_index += 1;
                            let mut msg_id = parsed.as_ref().map(|(id, _)| id.clone()).unwrap_or(fallback);
                            if msg_id.len() > 64 {
                                msg_id = format!("msg_{}", short_hash(&msg_id));
                            }
                            let mut item = json!({
                                "type": "message",
                                "role": "assistant",
                                "content": [{ "type": "output_text", "text": sanitize_surrogates(&text.text), "annotations": [] }],
                                "status": "completed",
                                "id": msg_id
                            });
                            if let Some((_, Some(phase))) = parsed {
                                item["phase"] = json!(phase);
                            }
                            output.push(item);
                        }
                        AssistantContentBlock::ToolCall(tc) => {
                            let parts: Vec<&str> = tc.id.splitn(2, '|').collect();
                            let call_id = parts[0];
                            let mut item_id = parts.get(1).map(|s| s.to_string());
                            if is_different_model && item_id.as_deref().map(|s| s.starts_with("fc_")) == Some(true) {
                                item_id = None;
                            }
                            output.push(json!({
                                "type": "function_call",
                                "id": item_id,
                                "call_id": call_id,
                                "name": tc.name,
                                "arguments": tc.arguments.to_string()
                            }));
                        }
                    }
                }
                if !output.is_empty() {
                    messages.extend(output);
                }
            }
            Message::ToolResult {
                tool_call_id, content, ..
            } => {
                let text_result: String = content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let has_images = content.iter().any(|b| matches!(b, ContentBlock::Image { .. }));
                let has_text = !text_result.is_empty();
                let call_id = tool_call_id.split('|').next().unwrap_or(&tool_call_id);
                let output_val = if has_images && model.input.iter().any(|i| i == "image") {
                    let mut parts = Vec::new();
                    if has_text {
                        parts.push(json!({ "type": "input_text", "text": sanitize_surrogates(&text_result) }));
                    }
                    for b in &content {
                        if let ContentBlock::Image { data, mime_type } = b {
                            parts.push(json!({
                                "type": "input_image",
                                "detail": "auto",
                                "image_url": format!("data:{mime_type};base64,{data}")
                            }));
                        }
                    }
                    Value::Array(parts)
                } else {
                    json!(sanitize_surrogates(if has_text {
                        text_result.as_str()
                    } else if has_images {
                        "(see attached image)"
                    } else {
                        "(no tool output)"
                    }))
                };
                messages.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": output_val
                }));
            }
        }
    }
    messages
}

pub fn convert_responses_tools(tools: &[crate::types::Tool], strict: Option<bool>) -> Vec<Value> {
    let strict = strict.unwrap_or(false);
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
                "strict": strict
            })
        })
        .collect()
}

struct StreamingToolCall {
    tool_call: ToolCall,
    partial_json: String,
}

enum OutputSlot {
    Thinking {
        block: ThinkingContent,
        content_index: usize,
    },
    Text {
        block: TextContent,
        content_index: usize,
    },
    ToolCall {
        block: StreamingToolCall,
        content_index: usize,
    },
}

fn create_output_slot(
    output_index: usize,
    item: &Value,
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    output_slots: &mut HashMap<usize, OutputSlot>,
) {
    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match item_type {
        "reasoning" => {
            let block = ThinkingContent::new("");
            output.content.push(AssistantContentBlock::Thinking(block.clone()));
            let content_index = output.content.len() - 1;
            stream.push(AssistantMessageEvent::ThinkingStart {
                content_index,
                partial: output.clone(),
            });
            output_slots.insert(output_index, OutputSlot::Thinking { block, content_index });
        }
        "message" => {
            let block = TextContent::new("");
            output.content.push(AssistantContentBlock::Text(block.clone()));
            let content_index = output.content.len() - 1;
            stream.push(AssistantMessageEvent::TextStart {
                content_index,
                partial: output.clone(),
            });
            output_slots.insert(output_index, OutputSlot::Text { block, content_index });
        }
        "function_call" => {
            let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");
            let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = item.get("arguments").and_then(|v| v.as_str()).unwrap_or("");
            let block = StreamingToolCall {
                tool_call: ToolCall::new(format!("{call_id}|{id}"), name, parse_streaming_json(Some(args))),
                partial_json: args.to_string(),
            };
            output
                .content
                .push(AssistantContentBlock::ToolCall(block.tool_call.clone()));
            let content_index = output.content.len() - 1;
            stream.push(AssistantMessageEvent::ToolcallStart {
                content_index,
                partial: output.clone(),
            });
            output_slots.insert(output_index, OutputSlot::ToolCall { block, content_index });
        }
        _ => {}
    }
}

#[derive(Default)]
pub struct ResponsesStreamState {
    pub saw_terminal: bool,
    output_slots: HashMap<usize, OutputSlot>,
}

pub fn process_responses_stream_event(
    event: &Value,
    state: &mut ResponsesStreamState,
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    model: &Model,
    options: Option<&OpenAIResponsesStreamOptions>,
) -> Result<()> {
    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match event_type {
        "response.created" => {
            if let Some(id) = event.pointer("/response/id").and_then(|v| v.as_str()) {
                output.response_id = Some(id.to_string());
            }
        }
        "response.output_item.added" => {
            let idx = event.get("output_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if let Some(item) = event.get("item") {
                create_output_slot(idx, item, output, stream, &mut state.output_slots);
            }
        }
        "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
            let idx = event.get("output_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let delta = event.get("delta").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(OutputSlot::Thinking { block, content_index }) = state.output_slots.get_mut(&idx) {
                block.thinking.push_str(delta);
                if let Some(AssistantContentBlock::Thinking(t)) = output.content.get_mut(*content_index) {
                    t.thinking = block.thinking.clone();
                }
                stream.push(AssistantMessageEvent::ThinkingDelta {
                    content_index: *content_index,
                    delta: delta.to_string(),
                    partial: output.clone(),
                });
            }
        }
        "response.output_text.delta" | "response.refusal.delta" => {
            let idx = event.get("output_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let delta = event.get("delta").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(OutputSlot::Text { block, content_index }) = state.output_slots.get_mut(&idx) {
                block.text.push_str(delta);
                if let Some(AssistantContentBlock::Text(t)) = output.content.get_mut(*content_index) {
                    t.text = block.text.clone();
                }
                stream.push(AssistantMessageEvent::TextDelta {
                    content_index: *content_index,
                    delta: delta.to_string(),
                    partial: output.clone(),
                });
            }
        }
        "response.function_call_arguments.delta" => {
            let idx = event.get("output_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let delta = event.get("delta").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(OutputSlot::ToolCall { block, content_index }) = state.output_slots.get_mut(&idx) {
                block.partial_json.push_str(delta);
                block.tool_call.arguments = parse_streaming_json(Some(&block.partial_json));
                if let Some(AssistantContentBlock::ToolCall(tc)) = output.content.get_mut(*content_index) {
                    tc.arguments = block.tool_call.arguments.clone();
                }
                stream.push(AssistantMessageEvent::ToolcallDelta {
                    content_index: *content_index,
                    delta: delta.to_string(),
                    partial: output.clone(),
                });
            }
        }
        "response.output_item.done" => {
            let idx = event.get("output_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if let Some(item) = event.get("item") {
                match state.output_slots.get_mut(&idx) {
                    Some(OutputSlot::Thinking { block, content_index }) => {
                        if let Some(summary) = item.get("summary").and_then(|v| v.as_array()) {
                            let text: String = summary
                                .iter()
                                .filter_map(|s| s.get("text").and_then(|t| t.as_str()))
                                .collect::<Vec<_>>()
                                .join("\n\n");
                            if !text.is_empty() {
                                block.thinking = text;
                            }
                        }
                        block.thinking_signature = Some(item.to_string());
                        if let Some(AssistantContentBlock::Thinking(t)) = output.content.get_mut(*content_index) {
                            t.thinking = block.thinking.clone();
                            t.thinking_signature = block.thinking_signature.clone();
                        }
                        stream.push(AssistantMessageEvent::ThinkingEnd {
                            content_index: *content_index,
                            content: block.thinking.clone(),
                            partial: output.clone(),
                        });
                        state.output_slots.remove(&idx);
                    }
                    Some(OutputSlot::Text { block, content_index }) => {
                        if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                            block.text = content
                                .iter()
                                .filter_map(|c| {
                                    if c.get("type")?.as_str()? == "output_text" {
                                        c.get("text")?.as_str()
                                    } else {
                                        c.get("refusal")?.as_str()
                                    }
                                })
                                .collect::<String>();
                        }
                        if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                            let phase = item.get("phase").and_then(|v| v.as_str());
                            block.text_signature = Some(encode_text_signature_v1(id, phase));
                        }
                        if let Some(AssistantContentBlock::Text(t)) = output.content.get_mut(*content_index) {
                            t.text = block.text.clone();
                            t.text_signature = block.text_signature.clone();
                        }
                        stream.push(AssistantMessageEvent::TextEnd {
                            content_index: *content_index,
                            content: block.text.clone(),
                            partial: output.clone(),
                        });
                        state.output_slots.remove(&idx);
                    }
                    Some(OutputSlot::ToolCall { block, content_index }) => {
                        if let Some(args) = item.get("arguments").and_then(|v| v.as_str()) {
                            block.partial_json = args.to_string();
                        }
                        block.tool_call.arguments = parse_streaming_json(Some(&block.partial_json));
                        if let Some(AssistantContentBlock::ToolCall(tc)) = output.content.get_mut(*content_index) {
                            tc.arguments = block.tool_call.arguments.clone();
                        }
                        stream.push(AssistantMessageEvent::ToolcallEnd {
                            content_index: *content_index,
                            tool_call: block.tool_call.clone(),
                            partial: output.clone(),
                        });
                        state.output_slots.remove(&idx);
                    }
                    None => {
                        create_output_slot(idx, item, output, stream, &mut state.output_slots);
                    }
                }
            }
        }
        "response.completed" | "response.incomplete" => {
            state.saw_terminal = true;
            if let Some(response) = event.get("response") {
                if let Some(id) = response.get("id").and_then(|v| v.as_str()) {
                    output.response_id = Some(id.to_string());
                }
                if let Some(usage) = response.get("usage") {
                    let cached = usage
                        .pointer("/input_tokens_details/cached_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    output.usage.input = input.saturating_sub(cached);
                    output.usage.output = output_tokens;
                    output.usage.cache_read = cached;
                    output.usage.reasoning = usage
                        .pointer("/output_tokens_details/reasoning_tokens")
                        .and_then(|v| v.as_u64());
                    output.usage.total_tokens = usage.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    calculate_cost(model, &mut output.usage);
                    if let Some(opts) = &options
                        && let Some(apply) = &opts.apply_service_tier_pricing
                    {
                        let tier = response.get("service_tier").and_then(|v| v.as_str());
                        apply(&mut output.usage, tier);
                    }
                }
                output.stop_reason = map_stop_reason(response.get("status").and_then(|v| v.as_str()));
                if output.content.iter().any(|b| b.is_tool_call()) && output.stop_reason == StopReason::Stop {
                    output.stop_reason = StopReason::ToolUse;
                }
            }
        }
        "error" => {
            let code = event.get("code").and_then(|v| v.as_str()).unwrap_or("unknown");
            let message = event.get("message").and_then(|v| v.as_str()).unwrap_or("unknown");
            return Err(anyhow!("Error Code {code}: {message}"));
        }
        "response.failed" => {
            let err = event.pointer("/response/error");
            let code = err
                .and_then(|e| e.get("code"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let message = err
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow!("{code}: {message}"));
        }
        _ => {}
    }
    Ok(())
}

pub async fn process_responses_stream(
    events: Vec<Value>,
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    model: &Model,
    options: Option<OpenAIResponsesStreamOptions>,
) -> Result<()> {
    let mut state = ResponsesStreamState::default();
    for event in events {
        process_responses_stream_event(&event, &mut state, output, stream, model, options.as_ref())?;
    }
    if !state.saw_terminal {
        return Err(anyhow!(
            "OpenAI Responses stream ended before a terminal response event"
        ));
    }
    Ok(())
}

fn map_stop_reason(status: Option<&str>) -> StopReason {
    match status {
        Some("completed") => StopReason::Stop,
        Some("incomplete") => StopReason::Length,
        Some("failed") | Some("cancelled") => StopReason::Error,
        _ => StopReason::Stop,
    }
}
