use elph_ai::api::openai_completions::{OpenAICompletionsOptions, build_openai_completions_params};
use elph_ai::types::{Context, Message, OpenAICompletionsCompat, Tool, UserContent};
use elph_ai::types::{Model, ModelCost};
use serde_json::json;

fn cache_control_model() -> Model {
    Model {
        id: "custom-qwen".to_string(),
        name: "Custom Qwen".to_string(),
        api: "openai-completions".to_string(),
        provider: "openrouter".to_string(),
        base_url: "https://example.com/v1".to_string(),
        reasoning: true,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,

            tiers: None,
        },
        context_window: 128_000,
        max_tokens: 32_000,
        headers: None,
        openai_completions_compat: Some(OpenAICompletionsCompat {
            cache_control_format: Some("anthropic".to_string()),
            supports_store: None,
            supports_developer_role: None,
            supports_reasoning_effort: None,
            supports_usage_in_streaming: None,
            max_tokens_field: None,
            requires_tool_result_name: None,
            requires_assistant_after_tool_result: None,
            requires_thinking_as_text: None,
            requires_reasoning_content_on_assistant_messages: None,
            thinking_format: None,
            zai_tool_stream: None,
            supports_strict_mode: None,
            send_session_affinity_headers: None,
            supports_long_cache_retention: None,
        }),
        openai_responses_compat: None,
        anthropic_compat: None,
    }
}

fn sample_context() -> Context {
    Context {
        system_prompt: Some("System prompt".to_string()),
        messages: vec![Message::User {
            content: UserContent::Text("Hello".to_string()),
            timestamp: 0,
        }],
        tools: Some(vec![Tool {
            name: "read".to_string(),
            description: "Read a file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        }]),
    }
}

#[test]
fn applies_anthropic_cache_markers_when_compat_enables_them() {
    let params = build_openai_completions_params(
        &cache_control_model(),
        &sample_context(),
        &OpenAICompletionsOptions::default(),
    )
    .expect("params");

    let instruction = params["messages"]
        .as_array()
        .and_then(|msgs| msgs.iter().find(|m| m["role"] == "system" || m["role"] == "developer"))
        .expect("instruction message");
    let content = instruction["content"].as_array().expect("array content");
    assert_eq!(content[0]["cache_control"]["type"], "ephemeral");

    let tools = params["tools"].as_array().expect("tools");
    assert_eq!(tools.last().unwrap()["cache_control"]["type"], "ephemeral");

    let last = params["messages"].as_array().unwrap().last().unwrap();
    assert_eq!(last["role"], "user");
    let last_content = last["content"].as_array().expect("user content array");
    assert_eq!(last_content[0]["cache_control"]["type"], "ephemeral");
}
