use serde_json::{Value, json};

use crate::types::{AssistantContentBlock, ContentBlock, Context, Message, Model, StopReason, UserContent};
use crate::utils::sanitize_unicode::sanitize_surrogates;

use super::transform_messages::transform_messages;

pub type GoogleThinkingLevel = &'static str; // "THINKING_LEVEL_UNSPECIFIED" | "MINIMAL" | "LOW" | "MEDIUM" | "HIGH"

pub fn is_thinking_part(part: &Value) -> bool {
    part.get("thought").and_then(|v| v.as_bool()) == Some(true)
}

pub fn retain_thought_signature(existing: Option<&str>, incoming: Option<&str>) -> Option<String> {
    if let Some(incoming) = incoming
        && !incoming.is_empty()
    {
        return Some(incoming.to_string());
    }
    existing.map(|s| s.to_string())
}

pub fn requires_tool_call_id(model_id: &str) -> bool {
    model_id.starts_with("claude-") || model_id.starts_with("gpt-oss-")
}

fn get_gemini_major_version(model_id: &str) -> Option<u32> {
    let lower = model_id.to_lowercase();
    let re = regex::Regex::new(r"^gemini(?:-live)?-(\d+)").ok()?;
    let caps = re.captures(&lower)?;
    caps.get(1)?.as_str().parse().ok()
}

fn supports_multimodal_function_response(model_id: &str) -> bool {
    if let Some(major) = get_gemini_major_version(model_id) {
        return major >= 3;
    }
    true
}

pub fn convert_messages(model: &Model, context: &Context) -> Vec<Value> {
    let mut contents = Vec::new();
    let normalize = |id: &str| -> String {
        if !requires_tool_call_id(&model.id) {
            return id.to_string();
        }
        let sanitized: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        sanitized.chars().take(64).collect()
    };

    let transformed = transform_messages(context.messages.clone(), model, |id, _m, _src| normalize(id));

    for msg in transformed {
        match msg {
            Message::User { content, .. } => {
                let parts = match content {
                    UserContent::Text(text) => vec![json!({ "text": sanitize_surrogates(&text) })],
                    UserContent::Blocks(blocks) => blocks
                        .into_iter()
                        .map(|b| match b {
                            ContentBlock::Text { text } => json!({ "text": sanitize_surrogates(&text) }),
                            ContentBlock::Image { data, mime_type } => json!({
                                "inlineData": { "mimeType": mime_type, "data": data }
                            }),
                        })
                        .collect(),
                };
                if parts.is_empty() {
                    continue;
                }
                contents.push(json!({ "role": "user", "parts": parts }));
            }
            Message::Assistant(assistant) => {
                let is_same = assistant.provider == model.provider && assistant.model == model.id;
                let mut parts = Vec::new();
                for block in &assistant.content {
                    match block {
                        AssistantContentBlock::Text(t) => {
                            if t.text.trim().is_empty() {
                                continue;
                            }
                            let mut part = json!({ "text": sanitize_surrogates(&t.text) });
                            if let Some(sig) = resolve_thought_signature(is_same, t.text_signature.as_deref()) {
                                part["thoughtSignature"] = json!(sig);
                            }
                            parts.push(part);
                        }
                        AssistantContentBlock::Thinking(t) => {
                            if t.thinking.trim().is_empty() {
                                continue;
                            }
                            if is_same {
                                let mut part = json!({
                                    "thought": true,
                                    "text": sanitize_surrogates(&t.thinking)
                                });
                                if let Some(sig) = resolve_thought_signature(is_same, t.thinking_signature.as_deref()) {
                                    part["thoughtSignature"] = json!(sig);
                                }
                                parts.push(part);
                            } else {
                                parts.push(json!({ "text": sanitize_surrogates(&t.thinking) }));
                            }
                        }
                        AssistantContentBlock::ToolCall(tc) => {
                            let mut fc = json!({
                                "name": tc.name,
                                "args": tc.arguments
                            });
                            if requires_tool_call_id(&model.id) {
                                fc["id"] = json!(tc.id);
                            }
                            let mut part = json!({ "functionCall": fc });
                            if let Some(sig) = resolve_thought_signature(is_same, tc.thought_signature.as_deref()) {
                                part["thoughtSignature"] = json!(sig);
                            }
                            parts.push(part);
                        }
                    }
                }
                if parts.is_empty() {
                    continue;
                }
                contents.push(json!({ "role": "model", "parts": parts }));
            }
            Message::ToolResult {
                tool_name,
                tool_call_id,
                content,
                is_error,
                ..
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
                let response_value = if has_text {
                    sanitize_surrogates(&text_result)
                } else if has_images {
                    "(see attached image)".to_string()
                } else {
                    String::new()
                };

                let image_parts: Vec<Value> = content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Image { data, mime_type } if model.input.iter().any(|i| i == "image") => {
                            Some(json!({ "inlineData": { "mimeType": mime_type, "data": data } }))
                        }
                        _ => None,
                    })
                    .collect();

                let multimodal = supports_multimodal_function_response(&model.id);
                let mut fr = json!({
                    "name": tool_name,
                    "response": if is_error {
                        json!({ "error": response_value })
                    } else {
                        json!({ "output": response_value })
                    }
                });
                if has_images && multimodal {
                    fr["parts"] = json!(image_parts);
                }
                if requires_tool_call_id(&model.id) {
                    fr["id"] = json!(tool_call_id);
                }
                let part = json!({ "functionResponse": fr });

                if let Some(last) = contents.last_mut() {
                    if last.get("role") == Some(&json!("user"))
                        && last
                            .get("parts")
                            .and_then(|p| p.as_array())
                            .map(|a| a.iter().any(|p| p.get("functionResponse").is_some()))
                            == Some(true)
                    {
                        last["parts"].as_array_mut().unwrap().push(part);
                    } else {
                        contents.push(json!({ "role": "user", "parts": [part] }));
                    }
                } else {
                    contents.push(json!({ "role": "user", "parts": [part] }));
                }

                if has_images && !multimodal {
                    contents.push(json!({
                        "role": "user",
                        "parts": [{ "text": "Tool result image:" }, image_parts]
                    }));
                }
            }
        }
    }
    contents
}

fn resolve_thought_signature(is_same: bool, signature: Option<&str>) -> Option<String> {
    if !is_same {
        return None;
    }
    let sig = signature?;
    if sig.len() % 4 != 0 {
        return None;
    }
    if sig
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
    {
        Some(sig.to_string())
    } else {
        None
    }
}

const JSON_SCHEMA_META: &[&str] = &[
    "$schema",
    "$id",
    "$anchor",
    "$dynamicAnchor",
    "$vocabulary",
    "$comment",
    "$defs",
    "definitions",
];

fn sanitize_for_openapi(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut result = serde_json::Map::new();
            for (k, v) in map {
                if JSON_SCHEMA_META.contains(&k.as_str()) {
                    continue;
                }
                result.insert(k.clone(), sanitize_for_openapi(v));
            }
            Value::Object(result)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sanitize_for_openapi).collect()),
        other => other.clone(),
    }
}

pub fn convert_tools(tools: &[crate::types::Tool], use_parameters: bool) -> Option<Vec<Value>> {
    if tools.is_empty() {
        return None;
    }
    let decls: Vec<Value> = tools
        .iter()
        .map(|tool| {
            let mut decl = json!({
                "name": tool.name,
                "description": tool.description,
            });
            if use_parameters {
                decl["parameters"] = sanitize_for_openapi(&tool.parameters);
            } else {
                decl["parametersJsonSchema"] = tool.parameters.clone();
            }
            decl
        })
        .collect();
    Some(vec![json!({ "functionDeclarations": decls })])
}

pub fn map_tool_choice(choice: &str) -> &'static str {
    match choice {
        "auto" => "AUTO",
        "none" => "NONE",
        "any" => "ANY",
        _ => "AUTO",
    }
}

pub fn map_stop_reason_string(reason: &str) -> StopReason {
    match reason {
        "STOP" => StopReason::Stop,
        "MAX_TOKENS" => StopReason::Length,
        _ => StopReason::Error,
    }
}

pub fn map_stop_reason_finish(finish: &str) -> StopReason {
    match finish {
        "STOP" | "FINISH_REASON_UNSPECIFIED" => StopReason::Stop,
        "MAX_TOKENS" => StopReason::Length,
        _ => StopReason::Error,
    }
}
