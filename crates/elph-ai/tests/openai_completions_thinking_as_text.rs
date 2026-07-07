use elph_ai::api::openai_compat::ResolvedOpenAICompletionsCompat;
use elph_ai::api::openai_completions::convert_messages;
use elph_ai::types::{
    AssistantContentBlock, AssistantMessage, Context, Message, Model, ModelCost, ThinkingContent, UserContent,
};

fn thinking_as_text_compat() -> ResolvedOpenAICompletionsCompat {
    ResolvedOpenAICompletionsCompat {
        supports_store: true,
        supports_developer_role: true,
        supports_reasoning_effort: true,
        supports_usage_in_streaming: true,
        max_tokens_field: "max_completion_tokens".to_string(),
        requires_tool_result_name: false,
        requires_assistant_after_tool_result: false,
        requires_thinking_as_text: true,
        requires_reasoning_content_on_assistant_messages: false,
        thinking_format: "openai".to_string(),
        zai_tool_stream: false,
        supports_strict_mode: true,
        cache_control_format: None,
        send_session_affinity_headers: false,
        supports_long_cache_retention: true,
    }
}

fn model() -> Model {
    Model {
        id: "repro-model".to_string(),
        name: "Repro".to_string(),
        api: "openai-completions".to_string(),
        provider: "repro".to_string(),
        base_url: "http://127.0.0.1:1".to_string(),
        reasoning: true,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 128_000,
        max_tokens: 4096,
        headers: None,
        openai_completions_compat: None,
        openai_responses_compat: None,
        anthropic_compat: None,
    }
}

#[test]
fn converts_thinking_blocks_to_plain_text_content() {
    let compat = thinking_as_text_compat();
    let context = Context {
        system_prompt: None,
        messages: vec![
            Message::User {
                content: UserContent::Text("hello".to_string()),
                timestamp: 1,
            },
            Message::Assistant(AssistantMessage {
                role: "assistant".to_string(),
                content: vec![
                    AssistantContentBlock::Thinking({
                        let mut t = ThinkingContent::new("internal reasoning");
                        t.thinking_signature = Some("reasoning_content".to_string());
                        t
                    }),
                    AssistantContentBlock::Text(elph_ai::TextContent::new("visible")),
                ],
                api: "openai-completions".to_string(),
                provider: "repro".to_string(),
                model: "repro-model".to_string(),
                usage: Default::default(),
                stop_reason: elph_ai::StopReason::Stop,
                timestamp: 2,
                response_id: None,
                response_model: None,
                error_message: None,
            }),
        ],
        tools: None,
    };

    let messages = convert_messages(&model(), &context, &compat);
    let assistant = messages
        .iter()
        .find(|m| m.get("role") == Some(&serde_json::json!("assistant")))
        .expect("assistant");
    let content = assistant["content"].as_array().expect("content array");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "internal reasoning");
    assert_eq!(content[1]["type"], "text");
    assert_eq!(content[1]["text"], "visible");
}
