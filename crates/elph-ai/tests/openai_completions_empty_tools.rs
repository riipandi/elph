mod common;

use common::{completions_proxy_model, sample_user_context};
use elph_ai::api::openai_compat::has_tool_history;
use elph_ai::api::openai_completions::{OpenAICompletionsOptions, build_openai_completions_params};
use elph_ai::api::simple_options::clamp_max_tokens_to_context;
use elph_ai::get_builtin_model;
use elph_ai::types::{
    AssistantContentBlock, AssistantMessage, Context, Message, StopReason, ToolCall, Usage, UserContent,
};
use serde_json::json;

#[test]
fn omits_tools_field_when_context_tools_is_empty() {
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let mut context = sample_user_context(None);
    context.tools = Some(vec![]);
    let params =
        build_openai_completions_params(&model, &context, &OpenAICompletionsOptions::default()).expect("params");
    assert!(params.get("tools").is_none());
}

#[test]
fn omits_tools_field_when_context_tools_is_none() {
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let params =
        build_openai_completions_params(&model, &sample_user_context(None), &OpenAICompletionsOptions::default())
            .expect("params");
    assert!(params.get("tools").is_none());
}

#[test]
fn sends_empty_tools_array_when_conversation_has_tool_history() {
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let context = Context {
        system_prompt: None,
        messages: vec![
            Message::User {
                content: UserContent::Text("use the tool".to_string()),
                timestamp: 0,
            },
            Message::Assistant(AssistantMessage {
                role: "assistant".to_string(),
                content: vec![AssistantContentBlock::ToolCall(ToolCall {
                    kind: "toolCall".to_string(),
                    id: "t1".to_string(),
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
            }),
            Message::ToolResult {
                tool_call_id: "t1".to_string(),
                tool_name: "noop".to_string(),
                content: vec![elph_ai::types::ContentBlock::Text {
                    text: "done".to_string(),
                }],
                details: None,
                is_error: false,
                timestamp: 1,
            },
        ],
        tools: Some(vec![]),
    };
    assert!(has_tool_history(&context.messages));
    let params =
        build_openai_completions_params(&model, &context, &OpenAICompletionsOptions::default()).expect("params");
    assert_eq!(params["tools"], json!([]));
}

#[test]
fn uses_max_completion_tokens_for_openai_models() {
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let mut options = OpenAICompletionsOptions::default();
    options.base.max_tokens = Some(model.max_tokens);
    let params = build_openai_completions_params(&model, &sample_user_context(None), &options).expect("params");
    assert!(params.get("max_tokens").is_none());
    assert_eq!(params["max_completion_tokens"], model.max_tokens);
}

#[test]
fn clamps_default_max_tokens_to_remaining_context() {
    let mut model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    model.context_window = 10_000;
    model.max_tokens = 8000;
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text("x".repeat(8000)),
            timestamp: 0,
        }],
        tools: None,
    };
    let clamped = clamp_max_tokens_to_context(&model, &context, model.max_tokens);
    assert_eq!(clamped, 3904);
}

#[test]
fn cloudflare_ai_gateway_uses_max_tokens_field() {
    let model = get_builtin_model("cloudflare-ai-gateway", "workers-ai/@cf/moonshotai/kimi-k2.6").expect("model");
    let mut options = OpenAICompletionsOptions::default();
    options.base.max_tokens = Some(1234);
    let params =
        build_openai_completions_params(&model, &sample_user_context(Some("system")), &options).expect("params");
    assert_eq!(params["max_tokens"], 1234);
    assert!(params.get("max_completion_tokens").is_none());
    assert!(params.get("store").is_none());
    assert_eq!(params["messages"][0]["role"], "system");
}

#[test]
fn proxy_model_uses_conservative_openai_completions_fields() {
    let model = completions_proxy_model(Some(elph_ai::types::OpenAICompletionsCompat {
        supports_store: Some(false),
        ..Default::default()
    }));
    let mut options = OpenAICompletionsOptions::default();
    options.base.max_tokens = Some(1234);
    options.reasoning_effort = Some("high".to_string());
    let params =
        build_openai_completions_params(&model, &sample_user_context(Some("system")), &options).expect("params");
    assert_eq!(params["messages"][0]["role"], "system");
    assert!(params.get("store").is_none());
}
