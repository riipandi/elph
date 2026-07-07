mod common;

use common::{anthropic_model, sample_user_context};
use elph_ai::api::anthropic_messages::{AnthropicOptions, build_anthropic_messages_params};
use elph_ai::types::AnthropicMessagesCompat;
use serde_json::json;

#[test]
fn omits_thinking_when_disabled() {
    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            thinking_enabled: Some(false),
            ..Default::default()
        },
    )
    .expect("params");
    assert!(params.get("thinking").is_none());
}

#[test]
fn uses_budget_thinking_when_enabled() {
    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            thinking_enabled: Some(true),
            thinking_budget_tokens: Some(2048),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["thinking"]["type"], "enabled");
    assert_eq!(params["thinking"]["budget_tokens"], 2048);
}

#[test]
fn uses_adaptive_thinking_when_forced_by_compat() {
    let params = build_anthropic_messages_params(
        &anthropic_model(
            "https://api.anthropic.com",
            Some(AnthropicMessagesCompat {
                force_adaptive_thinking: Some(true),
                ..Default::default()
            }),
        ),
        &sample_user_context(Some("system")),
        &AnthropicOptions {
            thinking_enabled: Some(true),
            effort: Some("high".to_string()),
            ..Default::default()
        },
    )
    .expect("params");
    assert_eq!(params["thinking"]["type"], "adaptive");
    assert_eq!(params["thinking"]["effort"], "high");
}

#[test]
fn adds_eager_input_streaming_to_tools_by_default() {
    let mut context = sample_user_context(Some("system"));
    context.tools = Some(vec![elph_ai::types::Tool {
        name: "read".to_string(),
        description: "Read".to_string(),
        parameters: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } }
        }),
    }]);
    let params = build_anthropic_messages_params(
        &anthropic_model("https://api.anthropic.com", None),
        &context,
        &AnthropicOptions::default(),
    )
    .expect("params");
    assert_eq!(params["tools"][0]["eager_input_streaming"], true);
}

#[test]
fn omits_eager_input_streaming_when_compat_disables_it() {
    let mut context = sample_user_context(Some("system"));
    context.tools = Some(vec![elph_ai::types::Tool {
        name: "read".to_string(),
        description: "Read".to_string(),
        parameters: json!({ "type": "object", "properties": {} }),
    }]);
    let params = build_anthropic_messages_params(
        &anthropic_model(
            "https://api.anthropic.com",
            Some(AnthropicMessagesCompat {
                supports_eager_tool_input_streaming: Some(false),
                ..Default::default()
            }),
        ),
        &context,
        &AnthropicOptions::default(),
    )
    .expect("params");
    assert!(params["tools"][0].get("eager_input_streaming").is_none());
}
