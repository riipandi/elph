mod common;

use common::{anthropic_model, sample_user_context};
use elph_ai::api::anthropic_messages::{AnthropicOptions, build_anthropic_messages_params};
use elph_ai::get_builtin_model;
use elph_ai::types::AnthropicMessagesCompat;

#[test]
fn omits_temperature_for_claude_opus_4_7() {
    let model = get_builtin_model("anthropic", "claude-opus-4-7").expect("model");
    let params = build_anthropic_messages_params(
        &model,
        &sample_user_context(None),
        &AnthropicOptions {
            base: elph_ai::types::StreamOptions {
                temperature: Some(0.0),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("temperature").is_none());
}

#[test]
fn omits_temperature_for_claude_opus_4_8() {
    let model = get_builtin_model("anthropic", "claude-opus-4-8").expect("model");
    let params = build_anthropic_messages_params(
        &model,
        &sample_user_context(None),
        &AnthropicOptions {
            base: elph_ai::types::StreamOptions {
                temperature: Some(0.0),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("temperature").is_none());
}

#[test]
fn omits_default_temperature_for_claude_opus_4_7() {
    let model = get_builtin_model("anthropic", "claude-opus-4-7").expect("model");
    let params = build_anthropic_messages_params(
        &model,
        &sample_user_context(None),
        &AnthropicOptions {
            base: elph_ai::types::StreamOptions {
                temperature: Some(1.0),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("temperature").is_none());
}

#[test]
fn keeps_temperature_for_claude_opus_4_6() {
    let model = get_builtin_model("anthropic", "claude-opus-4-6").expect("model");
    let params = build_anthropic_messages_params(
        &model,
        &sample_user_context(None),
        &AnthropicOptions {
            base: elph_ai::types::StreamOptions {
                temperature: Some(0.0),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["temperature"], 0.0);
}

#[test]
fn keeps_temperature_for_claude_sonnet_4_6() {
    let model = get_builtin_model("anthropic", "claude-sonnet-4-6").expect("model");
    let params = build_anthropic_messages_params(
        &model,
        &sample_user_context(None),
        &AnthropicOptions {
            base: elph_ai::types::StreamOptions {
                temperature: Some(0.0),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["temperature"], 0.0);
}

#[test]
fn omits_temperature_when_compat_disables_it() {
    let params = build_anthropic_messages_params(
        &anthropic_model(
            "https://my-proxy.example.com/v1",
            Some(AnthropicMessagesCompat {
                supports_temperature: Some(false),
                ..Default::default()
            }),
        ),
        &sample_user_context(None),
        &AnthropicOptions {
            base: elph_ai::types::StreamOptions {
                temperature: Some(0.0),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("temperature").is_none());
}

#[test]
fn omits_temperature_when_thinking_is_enabled() {
    let model = get_builtin_model("anthropic", "claude-sonnet-4-6").expect("model");
    let params = build_anthropic_messages_params(
        &model,
        &sample_user_context(None),
        &AnthropicOptions {
            base: elph_ai::types::StreamOptions {
                temperature: Some(0.0),
                ..Default::default()
            },
            thinking_enabled: Some(true),
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("temperature").is_none());
}
