use crate::types::{AssistantContentBlock, AssistantMessage, ContentBlock, Message, Model, StopReason, ToolCall};

const NON_VISION_USER_IMAGE_PLACEHOLDER: &str = "(image omitted: model does not support images)";
const NON_VISION_TOOL_IMAGE_PLACEHOLDER: &str = "(tool image omitted: model does not support images)";

fn replace_images_with_placeholder(content: &[ContentBlock], placeholder: &str) -> Vec<ContentBlock> {
    let mut result = Vec::new();
    let mut previous_was_placeholder = false;
    for block in content {
        if matches!(block, ContentBlock::Image { .. }) {
            if !previous_was_placeholder {
                result.push(ContentBlock::Text {
                    text: placeholder.to_string(),
                });
            }
            previous_was_placeholder = true;
            continue;
        }
        if let ContentBlock::Text { text } = block {
            previous_was_placeholder = text == placeholder;
        }
        result.push(block.clone());
    }
    result
}

fn downgrade_unsupported_images(messages: Vec<Message>, model: &Model) -> Vec<Message> {
    if model.input.iter().any(|i| i == "image") {
        return messages;
    }
    messages
        .into_iter()
        .map(|msg| match msg {
            Message::User { content, timestamp } => match content {
                crate::types::UserContent::Text(_) => Message::User { content, timestamp },
                crate::types::UserContent::Blocks(blocks) => Message::User {
                    content: crate::types::UserContent::Blocks(replace_images_with_placeholder(
                        &blocks,
                        NON_VISION_USER_IMAGE_PLACEHOLDER,
                    )),
                    timestamp,
                },
            },
            Message::ToolResult {
                tool_call_id,
                tool_name,
                content,
                details,
                added_tool_names,
                is_error,
                timestamp,
            } => Message::ToolResult {
                tool_call_id,
                tool_name,
                content: replace_images_with_placeholder(&content, NON_VISION_TOOL_IMAGE_PLACEHOLDER),
                details,
                added_tool_names,
                is_error,
                timestamp,
            },
            other => other,
        })
        .collect()
}

/// Normalize tool call IDs and transform messages for cross-provider compatibility.
pub fn transform_messages<F>(messages: Vec<Message>, model: &Model, normalize_tool_call_id: F) -> Vec<Message>
where
    F: Fn(&str, &Model, &AssistantMessage) -> String,
{
    let mut tool_call_id_map = std::collections::HashMap::new();

    let normalized: Vec<Message> = messages
        .into_iter()
        .map(|msg| match msg {
            Message::User { content, timestamp } => Message::User { content, timestamp },
            Message::ToolResult {
                tool_call_id,
                tool_name,
                content,
                details,
                added_tool_names,
                is_error,
                timestamp,
            } => {
                let normalized_id = tool_call_id_map.get(&tool_call_id).cloned().unwrap_or(tool_call_id);
                Message::ToolResult {
                    tool_call_id: normalized_id,
                    tool_name,
                    content,
                    details,
                    added_tool_names,
                    is_error,
                    timestamp,
                }
            }
            Message::Assistant(mut assistant) => {
                let is_same_model =
                    assistant.provider == model.provider && assistant.api == model.api && assistant.model == model.id;
                let assistant_snapshot = assistant.clone();

                let mut transformed = Vec::new();
                for block in assistant.content.drain(..) {
                    match block {
                        AssistantContentBlock::Thinking(t) => {
                            if t.redacted == Some(true) {
                                if is_same_model {
                                    transformed.push(AssistantContentBlock::Thinking(t));
                                }
                                continue;
                            }
                            if is_same_model && t.thinking_signature.is_some() {
                                transformed.push(AssistantContentBlock::Thinking(t));
                                continue;
                            }
                            if t.thinking.trim().is_empty() {
                                continue;
                            }
                            if is_same_model {
                                transformed.push(AssistantContentBlock::Thinking(t));
                            } else {
                                transformed
                                    .push(AssistantContentBlock::Text(crate::types::TextContent::new(t.thinking)));
                            }
                        }
                        AssistantContentBlock::Text(text) => {
                            transformed.push(AssistantContentBlock::Text(text));
                        }
                        AssistantContentBlock::ToolCall(mut tc) => {
                            if !is_same_model {
                                tc.thought_signature = None;
                                let normalized_id = normalize_tool_call_id(&tc.id, model, &assistant_snapshot);
                                if normalized_id != tc.id {
                                    tool_call_id_map.insert(tc.id.clone(), normalized_id.clone());
                                    tc.id = normalized_id;
                                }
                            }
                            transformed.push(AssistantContentBlock::ToolCall(tc));
                        }
                    }
                }
                assistant.content = transformed;
                Message::Assistant(assistant)
            }
        })
        .collect();

    let image_aware = downgrade_unsupported_images(normalized, model);

    let mut result = Vec::new();
    let mut pending_tool_calls: Vec<ToolCall> = Vec::new();
    let mut existing_tool_result_ids = std::collections::HashSet::new();

    let insert_synthetic =
        |result: &mut Vec<Message>, pending: &mut Vec<ToolCall>, existing: &mut std::collections::HashSet<String>| {
            for tc in pending.drain(..) {
                if !existing.contains(&tc.id) {
                    result.push(Message::ToolResult {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        content: vec![ContentBlock::Text {
                            text: "No result provided".to_string(),
                        }],
                        details: None,
                        added_tool_names: None,
                        is_error: true,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    });
                }
            }
            existing.clear();
        };

    for msg in image_aware {
        match &msg {
            Message::Assistant(assistant) => {
                insert_synthetic(&mut result, &mut pending_tool_calls, &mut existing_tool_result_ids);
                if matches!(assistant.stop_reason, StopReason::Error | StopReason::Aborted) {
                    continue;
                }
                let tool_calls: Vec<ToolCall> = assistant
                    .content
                    .iter()
                    .filter_map(|b| {
                        if let AssistantContentBlock::ToolCall(tc) = b {
                            Some(tc.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                if !tool_calls.is_empty() {
                    pending_tool_calls = tool_calls;
                    existing_tool_result_ids.clear();
                }
                result.push(msg);
            }
            Message::ToolResult { tool_call_id, .. } => {
                existing_tool_result_ids.insert(tool_call_id.clone());
                result.push(msg);
            }
            Message::User { .. } => {
                insert_synthetic(&mut result, &mut pending_tool_calls, &mut existing_tool_result_ids);
                result.push(msg);
            }
        }
    }

    insert_synthetic(&mut result, &mut pending_tool_calls, &mut existing_tool_result_ids);
    result
}
