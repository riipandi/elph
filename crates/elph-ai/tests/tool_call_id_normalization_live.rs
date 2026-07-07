//! Live cross-provider tool call ID normalization tests mirroring pi-ai issue #1022.
//! Run with: `cargo test -p elph-ai --test tool_call_id_normalization_live -- --ignored`

mod common;

use elph_ai::types::{
    AssistantContentBlock, AssistantMessage, ContentBlock, Message, SimpleStreamOptions, StopReason, StreamOptions,
    Tool, ToolCall, Usage, UserContent,
};
use elph_ai::{builtin_models, get_builtin_model};
use serde_json::json;

fn has_env(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty())
}

fn echo_tool() -> Tool {
    Tool {
        name: "echo".to_string(),
        description: "Echoes the message back".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Message to echo back" }
            },
            "required": ["message"]
        }),
    }
}

const FAILING_TOOL_CALL_ID: &str = "call_pAYbIr76hXIjncD9UE4eGfnS|t5nnb2qYMFWGSsr13fhCd1CaCu3t3qONEPuOudu4HSVEtA8YJSL6FAZUxvoOoD792VIJWl91g87EdqsCWp9krVsdBysQoDaf9lMCLb8BS4EYi4gQd5kBQBYLlgD71PYwvf+TbMD9J9/5OMD42oxSRj8H+vRf78/l2Xla33LWz4nOgsddBlbvabICRs8GHt5C9PK5keFtzyi3lsyVKNlfduK3iphsZqs4MLv4zyGJnvZo/+QzShyk5xnMSQX/f98+aEoNflEApCdEOXipipgeiNWnpFSHbcwmMkZoJhURNu+JEz3xCh1mrXeYoN5o+trLL3IXJacSsLYXDrYTipZZbJFRPAucgbnjYBC+/ZzJOfkwCs+Gkw7EoZR7ZQgJ8ma+9586n4tT4cI8DEhBSZsWMjrCt8dxKg==";

fn build_prefilled_messages() -> Vec<Message> {
    vec![
        Message::User {
            content: UserContent::Text("Use the echo tool to echo 'hello'".to_string()),
            timestamp: 0,
        },
        Message::Assistant(AssistantMessage {
            role: "assistant".to_string(),
            content: vec![AssistantContentBlock::ToolCall(ToolCall {
                kind: "toolCall".to_string(),
                id: FAILING_TOOL_CALL_ID.to_string(),
                name: "echo".to_string(),
                arguments: json!({ "message": "hello" }),
                thought_signature: None,
            })],
            api: "openai-responses".to_string(),
            provider: "github-copilot".to_string(),
            model: "gpt-5.2-codex".to_string(),
            usage: Usage::default(),
            stop_reason: StopReason::ToolUse,
            timestamp: 0,
            response_id: None,
            response_model: None,
            error_message: None,
        }),
        Message::ToolResult {
            tool_call_id: FAILING_TOOL_CALL_ID.to_string(),
            tool_name: "echo".to_string(),
            content: vec![ContentBlock::Text {
                text: "hello".to_string(),
            }],
            details: None,
            is_error: false,
            timestamp: 1,
        },
        Message::User {
            content: UserContent::Text("Say hi".to_string()),
            timestamp: 2,
        },
    ]
}

#[tokio::test]
#[ignore = "requires OPENROUTER_API_KEY"]
async fn openrouter_handles_prefilled_context_with_long_pipe_separated_ids() {
    assert!(has_env("OPENROUTER_API_KEY"));
    let models = builtin_models(None);
    let model = get_builtin_model("openrouter", "openai/gpt-5.2-codex").expect("model");
    let response = models
        .complete_simple(
            &model,
            &elph_ai::types::Context {
                system_prompt: Some("You are a helpful assistant.".to_string()),
                messages: build_prefilled_messages(),
                tools: Some(vec![echo_tool()]),
            },
            Some(SimpleStreamOptions {
                base: StreamOptions {
                    api_key: std::env::var("OPENROUTER_API_KEY").ok(),
                    ..Default::default()
                },
                reasoning: None,
                thinking_budgets: None,
            }),
        )
        .await;

    assert_ne!(
        response.stop_reason,
        StopReason::Error,
        "{}",
        response.error_message.unwrap_or_default()
    );
    if let Some(error) = &response.error_message {
        assert!(!error.contains("call_id"));
        assert!(!error.contains("too long"));
    }
}
