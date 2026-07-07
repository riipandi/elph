mod common;

use common::anthropic_model;
use elph_ai::api::anthropic_messages::{AnthropicOptions, build_anthropic_messages_params};
use elph_ai::types::{
    AnthropicMessagesCompat, AssistantContentBlock, AssistantMessage, Context, Message, Model, ModelCost, StopReason,
    ThinkingContent, Usage, UserContent,
};

fn xiaomi_ams_model(compat: Option<AnthropicMessagesCompat>) -> Model {
    Model {
        id: "mimo-v2.5-pro".to_string(),
        name: "MiMo-V2.5-Pro".to_string(),
        api: "anthropic-messages".to_string(),
        provider: "xiaomi-token-plan-ams".to_string(),
        base_url: "http://127.0.0.1:9/anthropic".to_string(),
        reasoning: true,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 1_048_576,
        max_tokens: 1024,
        headers: None,
        openai_completions_compat: None,
        openai_responses_compat: None,
        anthropic_compat: compat,
    }
}

fn context_with_thinking(signature: &str) -> Context {
    Context {
        system_prompt: None,
        messages: vec![
            Message::User {
                content: UserContent::Text("first".to_string()),
                timestamp: 0,
            },
            Message::Assistant(AssistantMessage {
                role: "assistant".to_string(),
                content: vec![AssistantContentBlock::Thinking(ThinkingContent {
                    kind: "thinking".to_string(),
                    thinking: "internal reasoning".to_string(),
                    thinking_signature: Some(signature.to_string()),
                    redacted: None,
                })],
                api: "anthropic-messages".to_string(),
                provider: "xiaomi-token-plan-ams".to_string(),
                model: "mimo-v2.5-pro".to_string(),
                usage: Usage::default(),
                stop_reason: StopReason::Stop,
                timestamp: 1,
                response_id: None,
                response_model: None,
                error_message: None,
            }),
            Message::User {
                content: UserContent::Text("second".to_string()),
                timestamp: 2,
            },
        ],
        tools: None,
    }
}

#[test]
fn converts_empty_signature_thinking_to_text_by_default() {
    let params = build_anthropic_messages_params(
        &anthropic_model("http://127.0.0.1:9/anthropic", None),
        &context_with_thinking(""),
        &AnthropicOptions::default(),
    )
    .expect("params");
    let assistant = params["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|m| m["role"] == "assistant")
        .expect("assistant");
    assert_eq!(assistant["content"][0]["type"], "text");
    assert_eq!(assistant["content"][0]["text"], "internal reasoning");
}

#[test]
fn preserves_empty_signature_thinking_when_compat_allows_it() {
    let params = build_anthropic_messages_params(
        &xiaomi_ams_model(Some(AnthropicMessagesCompat {
            allow_empty_signature: Some(true),
            ..Default::default()
        })),
        &context_with_thinking(" "),
        &AnthropicOptions::default(),
    )
    .expect("params");
    let assistant = params["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|m| m["role"] == "assistant")
        .expect("assistant");
    assert_eq!(assistant["content"][0]["type"], "thinking");
    assert_eq!(assistant["content"][0]["thinking"], "internal reasoning");
    assert_eq!(assistant["content"][0]["signature"], "");
}
