mod common;

use common::sample_user_context;
use elph_ai::api::bedrock_converse_stream::{BedrockOptions, build_bedrock_converse_body};
use elph_ai::get_builtin_model;
use elph_ai::types::{
    AssistantContentBlock, AssistantMessage, ContentBlock, Message, StopReason, TextContent, Usage, UserContent,
};
use elph_ai::utils::sanitize_unicode::sanitize_utf16_code_units;

fn bedrock_model() -> elph_ai::types::Model {
    get_builtin_model("amazon-bedrock", "us.anthropic.claude-sonnet-4-5-20250929-v1:0").expect("model")
}

fn lone_surrogate_text() -> String {
    String::from_utf16(&sanitize_utf16_code_units(&[0xD83D])).expect("sanitized lone surrogate is empty")
}

fn bedrock_options() -> BedrockOptions {
    BedrockOptions {
        base: elph_ai::types::StreamOptions {
            cache_retention: Some(elph_ai::types::CacheRetention::None),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn payload_messages(context: elph_ai::types::Context) -> Vec<serde_json::Value> {
    build_bedrock_converse_body(&bedrock_model(), &context, &bedrock_options()).expect("payload")["messages"]
        .as_array()
        .expect("messages")
        .clone()
}

#[test]
fn replaces_blank_user_string_content_with_placeholder() {
    let mut context = sample_user_context(None);
    context.messages = vec![Message::User {
        content: UserContent::Text("   ".to_string()),
        timestamp: 0,
    }];
    let messages = payload_messages(context);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["content"][0]["text"], "<empty>");
}

#[test]
fn filters_blank_user_text_blocks_when_other_content_remains() {
    let context = elph_ai::types::Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Blocks(vec![
                ContentBlock::Text { text: String::new() },
                ContentBlock::Text {
                    text: "hello".to_string(),
                },
            ]),
            timestamp: 0,
        }],
        tools: None,
    };
    let messages = payload_messages(context);
    assert_eq!(messages[0]["content"], serde_json::json!([{ "text": "hello" }]));
}

#[test]
fn replaces_user_content_emptied_by_surrogate_sanitization_with_placeholder() {
    let context = elph_ai::types::Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text(lone_surrogate_text()),
            timestamp: 0,
        }],
        tools: None,
    };
    let messages = payload_messages(context);
    assert_eq!(messages[0]["content"][0]["text"], "<empty>");
}

#[test]
fn skips_assistant_text_blocks_emptied_by_surrogate_sanitization() {
    let model = bedrock_model();
    let context = elph_ai::types::Context {
        system_prompt: None,
        messages: vec![Message::Assistant(AssistantMessage {
            role: "assistant".to_string(),
            content: vec![AssistantContentBlock::Text(TextContent::new(lone_surrogate_text()))],
            api: model.api.clone(),
            provider: model.provider.clone(),
            model: model.id.clone(),
            diagnostics: None,
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            timestamp: 0,
            response_id: None,
            response_model: None,
            error_message: None,
        })],
        tools: None,
    };
    let messages = payload_messages(context);
    assert!(messages.is_empty());
}

#[test]
fn replaces_blank_tool_result_content_with_placeholder() {
    let context = elph_ai::types::Context {
        system_prompt: None,
        messages: vec![Message::ToolResult {
            tool_call_id: "tool-1".to_string(),
            tool_name: "tool".to_string(),
            content: vec![ContentBlock::Text { text: String::new() }],
            details: None,
            added_tool_names: None,
            is_error: false,
            timestamp: 0,
        }],
        tools: None,
    };
    let messages = payload_messages(context);
    assert_eq!(
        messages[0]["content"][0]["toolResult"]["content"],
        serde_json::json!([{ "text": "<empty>" }])
    );
}

#[test]
fn skips_assistant_messages_with_only_empty_text_blocks() {
    let model = bedrock_model();
    let context = elph_ai::types::Context {
        system_prompt: None,
        messages: vec![Message::Assistant(AssistantMessage {
            role: "assistant".to_string(),
            content: vec![AssistantContentBlock::Text(TextContent::new("   ".to_string()))],
            api: model.api.clone(),
            provider: model.provider.clone(),
            model: model.id.clone(),
            diagnostics: None,
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            timestamp: 0,
            response_id: None,
            response_model: None,
            error_message: None,
        })],
        tools: None,
    };
    let messages = payload_messages(context);
    assert!(messages.is_empty());
}
