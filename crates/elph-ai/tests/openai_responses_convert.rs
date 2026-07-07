mod common;

use common::sample_user_context;
use elph_ai::api::openai_responses_shared::convert_responses_messages;
use elph_ai::get_builtin_model;
use elph_ai::types::{AssistantContentBlock, AssistantMessage, Message, StopReason, ToolCall, Usage};
use elph_ai::utils::hash::short_hash;
use serde_json::json;
use std::collections::HashSet;

const COPILOT_ITEM_ID: &str = "I9b95oN1wD/cHXKTw3PpRkL6KkCtzTJhUxMouMWYwHeTo2j3htzfSk7YPx2vifiIM4g3A8XXyOj8q4Bt6SLUG7gqY1E3ELkrkVQNHglRfUmWj84lqxJY+Puieb3VKyX0FB+83TUzn91cDMF/4gzt990IzqVrc+nIb9RRscRD070Du16q1glydVjWR0SBJsE6TbY/esOjFpqplogQqrajm1eI++f3eLi73R6q7hVusY0QbeFySVxABCjhN0lXB04caBe1rzHjYzul6MAXj7uq+0r17VLq+yrtyYhN12wkmFqHeqTyEei6EFPbMy24Nc+IbJlkP0OCg02W+gOnyBFcbi2ctvJFSOhSjt1CqBdqCnnhwUqXjbWiT0wh3DmLScRgTHmGkaI+oAcQQjfic65nxj+TnEkReA==";

#[test]
fn hashes_foreign_copilot_tool_item_ids_to_fc_prefix() {
    let model = get_builtin_model("openai-codex", "gpt-5.5").expect("model");
    let raw_id = format!("call_4VnzVawQXPB9MgYib7CiQFEY|{COPILOT_ITEM_ID}");
    let foreign = AssistantMessage {
        role: "assistant".to_string(),
        content: vec![AssistantContentBlock::ToolCall(ToolCall {
            kind: "toolCall".to_string(),
            id: raw_id.clone(),
            name: "edit".to_string(),
            arguments: json!({ "path": "src/styles/app.css" }),
            thought_signature: None,
        })],
        api: "openai-responses".to_string(),
        provider: "github-copilot".to_string(),
        model: "gpt-5.5".to_string(),
        usage: Usage::default(),
        stop_reason: StopReason::ToolUse,
        timestamp: 0,
        response_id: None,
        response_model: None,
        error_message: None,
    };
    let context = elph_ai::types::Context {
        system_prompt: Some("You are concise.".to_string()),
        messages: vec![
            Message::User {
                content: elph_ai::types::UserContent::Text("Use the tool.".to_string()),
                timestamp: 0,
            },
            Message::Assistant(foreign),
            Message::ToolResult {
                tool_call_id: raw_id,
                tool_name: "edit".to_string(),
                content: vec![elph_ai::types::ContentBlock::Text { text: "ok".to_string() }],
                details: None,
                is_error: false,
                timestamp: 1,
            },
        ],
        tools: None,
    };
    let providers: HashSet<String> = ["openai", "openai-codex", "opencode"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let items = convert_responses_messages(&model, &context, &providers, None);
    let tool_call = items
        .iter()
        .find(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call"))
        .expect("function_call");
    let expected_item_id = format!("fc_{}", short_hash(COPILOT_ITEM_ID));
    assert_eq!(
        tool_call.get("id").and_then(|v| v.as_str()),
        Some(expected_item_id.as_str())
    );
    assert!(expected_item_id.len() <= 64);
}

#[test]
fn preserves_native_openai_tool_call_ids() {
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let native = AssistantMessage {
        role: "assistant".to_string(),
        content: vec![AssistantContentBlock::ToolCall(ToolCall {
            kind: "toolCall".to_string(),
            id: "call_native".to_string(),
            name: "noop".to_string(),
            arguments: json!({}),
            thought_signature: None,
        })],
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        usage: Usage::default(),
        stop_reason: StopReason::ToolUse,
        timestamp: 0,
        response_id: None,
        response_model: None,
        error_message: None,
    };
    let context = elph_ai::types::Context {
        system_prompt: None,
        messages: vec![Message::Assistant(native)],
        tools: None,
    };
    let providers: HashSet<String> = ["openai"].into_iter().map(|s| s.to_string()).collect();
    let items = convert_responses_messages(&model, &context, &providers, None);
    let tool_call = items
        .iter()
        .find(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call"))
        .expect("function_call");
    assert_eq!(tool_call.get("call_id").and_then(|v| v.as_str()), Some("call_native"));
}

#[test]
fn converts_assistant_text_to_output_message() {
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let context = sample_user_context(Some("system"));
    let providers: HashSet<String> = HashSet::new();
    let items = convert_responses_messages(&model, &context, &providers, None);
    assert!(
        items
            .iter()
            .any(|item| item.get("role").and_then(|v| v.as_str()) == Some("user"))
    );
}
