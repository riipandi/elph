use elph_ai::api::openai_compat::get_compat;
use elph_ai::api::openai_completions::convert_messages;
use elph_ai::api::transform_messages::transform_messages;
use elph_ai::get_builtin_model;
use elph_ai::types::{
    AssistantContentBlock, AssistantMessage, ContentBlock, Message, StopReason, ToolCall, Usage, UserContent,
};
use serde_json::json;

fn assistant_with_tool_call(id: &str) -> AssistantMessage {
    AssistantMessage {
        role: "assistant".to_string(),
        content: vec![AssistantContentBlock::ToolCall(ToolCall {
            kind: "toolCall".to_string(),
            id: id.to_string(),
            name: "calculate".to_string(),
            arguments: json!({ "expression": "25 * 18" }),
            thought_signature: None,
        })],
        api: "openai-completions".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o-mini".to_string(),
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::ToolUse,
        timestamp: 0,
        response_id: None,
        response_model: None,
        error_message: None,
    }
}

#[test]
fn inserts_synthetic_tool_result_when_tool_call_has_no_matching_result() {
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let messages = vec![
        Message::User {
            content: UserContent::Text("Calculate 25 * 18".to_string()),
            timestamp: 0,
        },
        Message::Assistant(assistant_with_tool_call("call_1")),
        Message::User {
            content: UserContent::Text("Never mind, what is 2+2?".to_string()),
            timestamp: 1,
        },
    ];
    let transformed = transform_messages(messages, &model, |id, _, _| id.to_string());
    let synthetic = transformed
        .iter()
        .find(|msg| matches!(msg, Message::ToolResult { tool_call_id, is_error: true, .. } if tool_call_id == "call_1"))
        .expect("synthetic tool result");
    if let Message::ToolResult { content, is_error, .. } = synthetic {
        assert!(*is_error);
        match &content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "No result provided"),
            other => panic!("expected text block, got {other:?}"),
        }
    }
}

#[test]
fn normalizes_foreign_pipe_separated_tool_call_ids_for_cross_provider_handoff() {
    let model = get_builtin_model("openrouter", "openai/gpt-5.2-codex").expect("model");
    let long_item_id = "t5nnb2qYMFWGSsr13fhCd1CaCu3t3qONEPuOudu4HSVEtA8YJSL6FAZUxvoOoD792VIJWl91g87EdqsCWp9krVsdBysQoDaf9lMCLb8BS4EYi4gQd5kBQBYLlgD71PYwvf+TbMD9J9/5OMD42oxSRj8H+vRf78/l2Xla33LWz4nOgsddBlbvabICRs8GHt5C9PK5keFtzyi3lsyVKNlfduK3iphsZqs4MLv4zyGJnvZo/+QzShyk5xnMSQX/f98+aEoNflEApCdEOXipipgeiNWnpFSHbcwmMkZoJhURNu+JEz3xCh1mrXeYoN5o+trLL3IXJacSsLYXDrYTipZZbJFRPAucgbnjYBC+/ZzJOfkwCs+Gkw7EoZR7ZQgJ8ma+9586n4tT4cI8DEhBSZsWMjrCt8dxKg==";
    let raw_id = format!("call_pAYbIr76hXIjncD9UE4eGfnS|{long_item_id}");
    let foreign = AssistantMessage {
        role: "assistant".to_string(),
        content: vec![AssistantContentBlock::ToolCall(ToolCall {
            kind: "toolCall".to_string(),
            id: raw_id.clone(),
            name: "echo".to_string(),
            arguments: json!({ "message": "hello" }),
            thought_signature: None,
        })],
        api: "openai-responses".to_string(),
        provider: "github-copilot".to_string(),
        model: "gpt-5.2-codex".to_string(),
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::ToolUse,
        timestamp: 0,
        response_id: None,
        response_model: None,
        error_message: None,
    };
    let messages = vec![
        Message::User {
            content: UserContent::Text("echo hello".to_string()),
            timestamp: 0,
        },
        Message::Assistant(foreign),
        Message::ToolResult {
            tool_call_id: raw_id,
            tool_name: "echo".to_string(),
            content: vec![ContentBlock::Text {
                text: "hello".to_string(),
            }],
            details: None,
            added_tool_names: None,
            is_error: false,
            timestamp: 1,
        },
    ];
    let transformed = transform_messages(messages, &model, |id, _, _| {
        if id.contains('|') {
            let parts: Vec<&str> = id.splitn(2, '|').collect();
            let call_id = parts[0].chars().take(64).collect::<String>();
            format!("{call_id}|fc_normalized")
        } else {
            id.to_string()
        }
    });
    let tool_call_id = match transformed.iter().find_map(|msg| match msg {
        Message::ToolResult { tool_call_id, .. } => Some(tool_call_id.clone()),
        _ => None,
    }) {
        Some(id) => id,
        None => panic!("expected tool result"),
    };
    assert!(tool_call_id.contains('|'));
    assert!(tool_call_id.len() < 200);
    assert!(!tool_call_id.contains('+'));
    assert!(!tool_call_id.contains('/'));
}

#[test]
fn openrouter_completions_normalizes_long_pipe_separated_ids_from_issue_1022() {
    let model = get_builtin_model("openrouter", "openai/gpt-5.2-codex").expect("model");
    let long_item_id = "t5nnb2qYMFWGSsr13fhCd1CaCu3t3qONEPuOudu4HSVEtA8YJSL6FAZUxvoOoD792VIJWl91g87EdqsCWp9krVsdBysQoDaf9lMCLb8BS4EYi4gQd5kBQBYLlgD71PYwvf+TbMD9J9/5OMD42oxSRj8H+vRf78/l2Xla33LWz4nOgsddBlbvabICRs8GHt5C9PK5keFtzyi3lsyVKNlfduK3iphsZqs4MLv4zyGJnvZo/+QzShyk5xnMSQX/f98+aEoNflEApCdEOXipipgeiNWnpFSHbcwmMkZoJhURNu+JEz3xCh1mrXeYoN5o+trLL3IXJacSsLYXDrYTipZZbJFRPAucgbnjYBC+/ZzJOfkwCs+Gkw7EoZR7ZQgJ8ma+9586n4tT4cI8DEhBSZsWMjrCt8dxKg==";
    let raw_id = format!("call_pAYbIr76hXIjncD9UE4eGfnS|{long_item_id}");
    let context = elph_ai::types::Context {
        system_prompt: Some("You are helpful.".to_string()),
        messages: vec![
            Message::User {
                content: UserContent::Text("echo hello".to_string()),
                timestamp: 0,
            },
            Message::Assistant(AssistantMessage {
                role: "assistant".to_string(),
                content: vec![AssistantContentBlock::ToolCall(ToolCall {
                    kind: "toolCall".to_string(),
                    id: raw_id.clone(),
                    name: "echo".to_string(),
                    arguments: json!({ "message": "hello" }),
                    thought_signature: None,
                })],
                api: "openai-responses".to_string(),
                provider: "github-copilot".to_string(),
                model: "gpt-5.2-codex".to_string(),
                diagnostics: None,
                usage: Usage::default(),
                stop_reason: StopReason::ToolUse,
                timestamp: 0,
                response_id: None,
                response_model: None,
                error_message: None,
            }),
            Message::ToolResult {
                tool_call_id: raw_id,
                tool_name: "echo".to_string(),
                content: vec![ContentBlock::Text {
                    text: "hello".to_string(),
                }],
                details: None,
                added_tool_names: None,
                is_error: false,
                timestamp: 1,
            },
        ],
        tools: None,
    };
    let compat = get_compat(&model);
    let messages = convert_messages(&model, &context, &compat);
    let tool_message = messages
        .iter()
        .find(|msg| msg.get("role").and_then(|v| v.as_str()) == Some("tool"))
        .expect("tool message");
    let tool_call_id = tool_message["tool_call_id"].as_str().expect("tool_call_id");
    assert!(tool_call_id.len() <= 40);
    assert!(!tool_call_id.contains('|'));
    assert!(!tool_call_id.contains('+'));
    assert!(!tool_call_id.contains('/'));
}
