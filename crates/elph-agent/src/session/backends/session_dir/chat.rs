//! `chat_history.jsonl` line format.

use elph_ai::{AssistantContentBlock, ContentBlock, Message, ToolCall, UserContent};
use serde_json::{Value, json};

use crate::types::AgentMessage;

/// Convert a tree message entry into a chat history line.
pub fn tree_message_to_chat_line(message: &AgentMessage) -> Option<Value> {
    match message {
        AgentMessage::Llm(msg) => match msg.as_ref() {
            Message::User { content, .. } => Some(json!({
                "type": "user",
                "content": user_content_to_chat_json(content),
            })),
            Message::Assistant(assistant) => {
                let text = assistant_text(assistant);
                let tool_calls = assistant_tool_calls(assistant);
                if tool_calls.is_empty() {
                    Some(json!({
                        "type": "assistant",
                        "content": text,
                    }))
                } else {
                    Some(json!({
                        "type": "assistant",
                        "content": text,
                        "tool_calls": tool_calls,
                    }))
                }
            }
            _ => None,
        },
        _ => None,
    }
}

/// Extract plain user prompt text for `prompt_history.jsonl`.
pub fn user_prompt_text(message: &AgentMessage) -> Option<String> {
    let AgentMessage::Llm(msg) = message else {
        return None;
    };
    let Message::User { content, .. } = msg.as_ref() else {
        return None;
    };
    let text = user_content_plain(content);
    if text.is_empty() { None } else { Some(text) }
}

fn user_content_to_chat_json(content: &UserContent) -> Value {
    match content {
        UserContent::Text(text) => Value::String(text.clone()),
        UserContent::Blocks(blocks) => {
            let items: Vec<Value> = blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(json!({
                        "type": "text",
                        "text": text,
                    })),
                    ContentBlock::Image { .. } => None,
                })
                .collect();
            Value::Array(items)
        }
    }
}

fn user_content_plain(content: &UserContent) -> String {
    match content {
        UserContent::Text(text) => text.clone(),
        UserContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn assistant_text(assistant: &elph_ai::AssistantMessage) -> String {
    assistant
        .content
        .iter()
        .filter_map(|block| match block {
            AssistantContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn assistant_tool_calls(assistant: &elph_ai::AssistantMessage) -> Vec<Value> {
    assistant
        .content
        .iter()
        .filter_map(|block| match block {
            AssistantContentBlock::ToolCall(tc) => Some(tool_call_to_chat_json(tc)),
            _ => None,
        })
        .collect()
}

fn tool_call_to_chat_json(tc: &ToolCall) -> Value {
    json!({
        "id": tc.id,
        "name": tc.name,
        "arguments": tc.arguments.to_string(),
    })
}
