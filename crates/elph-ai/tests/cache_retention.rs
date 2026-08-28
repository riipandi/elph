mod common;

use std::collections::HashMap;

use common::stream_options_with_cache;
use common::{anthropic_model, completions_proxy_model, responses_model, sample_user_context};
use elph_ai::api::anthropic_messages::AnthropicOptions;
use elph_ai::api::anthropic_messages::build_anthropic_messages_params;
use elph_ai::api::openai_completions::OpenAICompletionsOptions;
use elph_ai::api::openai_completions::build_openai_completions_params;
use elph_ai::api::openai_responses::OpenAIResponsesOptions;
use elph_ai::api::openai_responses::build_openai_responses_params;
use elph_ai::types::{
    AnthropicMessagesCompat, AssistantContentBlock, CacheRetention, ContentBlock, Message, OpenAICompletionsCompat,
    OpenAIResponsesCompat, ToolCall,
};
#[test]
fn anthropic_uses_ephemeral_cache_control_by_default() {
    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base: stream_options_with_cache(CacheRetention::Short, None),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["system"][0]["cache_control"]["type"], "ephemeral");
    assert!(params["system"][0]["cache_control"].get("ttl").is_none());
}

#[test]
fn anthropic_uses_one_hour_ttl_when_cache_retention_is_long() {
    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base: stream_options_with_cache(CacheRetention::Long, None),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["system"][0]["cache_control"]["ttl"], "1h");
}

#[test]
fn anthropic_omits_ttl_when_long_cache_retention_is_unsupported() {
    let params = build_anthropic_messages_params(
        &anthropic_model(
            "https://my-proxy.example.com/v1",
            Some(AnthropicMessagesCompat {
                supports_long_cache_retention: Some(false),
                ..Default::default()
            }),
        ),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base: stream_options_with_cache(CacheRetention::Long, None),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["system"][0]["cache_control"]["type"], "ephemeral");
    assert!(params["system"][0]["cache_control"].get("ttl").is_none());
}

#[test]
fn anthropic_omits_cache_control_when_cache_retention_is_none() {
    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base: stream_options_with_cache(CacheRetention::None, None),
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params["system"][0].get("cache_control").is_none());
}

#[test]
fn anthropic_marks_a_trailing_tool_result_as_the_rolling_breakpoint() {
    let mut context = sample_user_context(Some("system"));
    context.messages.push(Message::Assistant(elph_ai::AssistantMessage {
        role: "assistant".into(),
        content: vec![AssistantContentBlock::ToolCall(ToolCall::new(
            "call-1",
            "read",
            serde_json::json!({}),
        ))],
        api: "anthropic-messages".into(),
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
        response_model: None,
        response_id: None,
        diagnostics: None,
        pending_stop_reason: None,
        usage: Default::default(),
        stop_reason: elph_ai::StopReason::ToolUse,
        error_message: None,
        timestamp: 0,
    }));
    context.messages.push(Message::ToolResult {
        tool_call_id: "call-1".into(),
        tool_name: "read".into(),
        content: vec![ContentBlock::Text { text: "result".into() }],
        details: None,
        is_error: false,
        usage: None,
        timestamp: 0,
        added_tool_names: None,
    });

    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &context,
        &AnthropicOptions {
            base: stream_options_with_cache(CacheRetention::Short, None),
            ..Default::default()
        },
    )
    .expect("params");
    let messages = params["messages"].as_array().expect("messages");
    let last_content = messages.last().and_then(|m| m["content"].as_array()).expect("content");
    assert_eq!(last_content[0]["type"], "tool_result");
    assert_eq!(last_content[0]["cache_control"]["type"], "ephemeral");
}

#[test]
fn openai_responses_sets_prompt_cache_retention_for_proxy_with_long_retention() {
    let params = build_openai_responses_params(
        &responses_model(
            "https://my-proxy.example.com/v1",
            Some(OpenAIResponsesCompat {
                supports_long_cache_retention: Some(true),
                ..Default::default()
            }),
        ),
        &sample_user_context(Some("system")),
        &OpenAIResponsesOptions {
            base: stream_options_with_cache(CacheRetention::Long, Some("session-2")),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["prompt_cache_key"], "session-2");
    assert_eq!(params["prompt_cache_retention"], "24h");
}

#[test]
fn openai_responses_omits_long_cache_when_compat_disables_it() {
    let params = build_openai_responses_params(
        &responses_model(
            "https://api.openai.com/v1",
            Some(OpenAIResponsesCompat {
                supports_long_cache_retention: Some(false),
                ..Default::default()
            }),
        ),
        &sample_user_context(Some("system")),
        &OpenAIResponsesOptions {
            base: stream_options_with_cache(CacheRetention::Long, Some("session-compat-false")),
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("prompt_cache_key").is_none());
    assert!(params.get("prompt_cache_retention").is_none());
}

#[test]
fn modern_openai_responses_uses_explicit_mode_to_disable_implicit_writes() {
    let mut model = responses_model("https://api.openai.com/v1", None);
    model.id = "gpt-5.6".into();
    model.name = "GPT-5.6".into();
    model.openai_responses_compat = Some(OpenAIResponsesCompat {
        supports_explicit_prompt_cache_mode: Some(true),
        ..Default::default()
    });
    let params = build_openai_responses_params(
        &model,
        &sample_user_context(Some("system")),
        &OpenAIResponsesOptions {
            base: stream_options_with_cache(CacheRetention::None, Some("should-not-be-sent")),
            ..Default::default()
        },
    )
    .expect("params");

    assert_eq!(params["prompt_cache_options"]["mode"], "explicit");
    assert!(params.get("prompt_cache_key").is_none());
    assert!(params.get("prompt_cache_retention").is_none());
}

#[test]
fn unknown_openai_responses_proxy_receives_no_cache_fields_by_default() {
    let mut model = responses_model("https://proxy.example/v1", None);
    model.provider = "custom-openai".into();
    let params = build_openai_responses_params(
        &model,
        &sample_user_context(Some("system")),
        &OpenAIResponsesOptions {
            base: stream_options_with_cache(CacheRetention::Short, Some("opaque-session")),
            ..Default::default()
        },
    )
    .expect("params");

    assert!(params.get("prompt_cache_key").is_none());
    assert!(params.get("prompt_cache_options").is_none());
    assert!(params.get("prompt_cache_retention").is_none());
}

#[test]
fn openai_completions_sets_prompt_cache_for_proxy_with_long_retention() {
    let params = build_openai_completions_params(
        &completions_proxy_model(Some(OpenAICompletionsCompat {
            supports_long_cache_retention: Some(true),
            ..Default::default()
        })),
        &sample_user_context(Some("system")),
        &OpenAICompletionsOptions {
            base: stream_options_with_cache(CacheRetention::Long, Some("session-completions")),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["prompt_cache_key"], "session-completions");
    assert_eq!(params["prompt_cache_retention"], "24h");
}

#[test]
fn openai_completions_omits_long_cache_when_compat_disables_it() {
    let params = build_openai_completions_params(
        &completions_proxy_model(Some(OpenAICompletionsCompat {
            supports_long_cache_retention: Some(false),
            ..Default::default()
        })),
        &sample_user_context(Some("system")),
        &OpenAICompletionsOptions {
            base: stream_options_with_cache(CacheRetention::Long, Some("session-completions-false")),
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("prompt_cache_key").is_none());
    assert!(params.get("prompt_cache_retention").is_none());
}

#[test]
fn elph_cache_retention_env_maps_to_long_ttl() {
    let mut env = HashMap::new();
    env.insert("ELPH_CACHE_RETENTION".to_string(), "long".to_string());
    let mut base = stream_options_with_cache(CacheRetention::Short, None);
    base.cache_retention = None;
    base.env = Some(env);

    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            base,
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["system"][0]["cache_control"]["ttl"], "1h");
}

#[test]
fn cache_retention_env_accepts_none_and_invalid_values_fall_back_to_short() {
    let mut env = HashMap::new();
    env.insert("ELPH_CACHE_RETENTION".to_string(), "none".to_string());
    let mut options = stream_options_with_cache(CacheRetention::Short, None);
    options.cache_retention = None;
    options.env = Some(env);
    assert_eq!(CacheRetention::resolve(&options), CacheRetention::None);

    options
        .env
        .as_mut()
        .expect("env")
        .insert("ELPH_CACHE_RETENTION".into(), "invalid".into());
    assert_eq!(CacheRetention::resolve(&options), CacheRetention::Short);
}
