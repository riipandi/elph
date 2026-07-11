mod common;

use elph_ai::api::openai_responses_shared::convert_responses_messages;
use elph_ai::get_builtin_model;
use elph_ai::types::{
    AssistantContentBlock, AssistantMessage, ContentBlock, Message, StopReason, ToolCall, Usage, UserContent,
};
use serde_json::json;
use std::collections::HashSet;

#[test]
fn uses_no_tool_output_placeholder_for_empty_tool_results_without_images() {
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let assistant = AssistantMessage {
        role: "assistant".to_string(),
        content: vec![AssistantContentBlock::ToolCall(ToolCall {
            kind: "toolCall".to_string(),
            id: "tool-1".to_string(),
            name: "bash".to_string(),
            arguments: json!({ "command": "true" }),
            thought_signature: None,
        })],
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::ToolUse,
        timestamp: 0,
        response_id: None,
        response_model: None,
        error_message: None,
    };
    let context = elph_ai::types::Context {
        system_prompt: None,
        messages: vec![
            Message::User {
                content: UserContent::Text("Run the command".to_string()),
                timestamp: 0,
            },
            Message::Assistant(assistant),
            Message::ToolResult {
                tool_call_id: "tool-1".to_string(),
                tool_name: "bash".to_string(),
                content: vec![ContentBlock::Text { text: String::new() }],
                details: None,
                added_tool_names: None,
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
    let output = items
        .iter()
        .find(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call_output"))
        .expect("function_call_output");
    assert_eq!(output["output"], "(no tool output)");
    assert!(!output["output"].as_str().unwrap_or("").contains("see attached image"));
}
