use elph_ai::api::bedrock_converse_stream::{BedrockOptions, build_bedrock_converse_body};
use elph_ai::get_builtin_model;
use elph_ai::types::{Context, Message, ThinkingLevel, UserContent};
use serde_json::Value;

fn make_context(system_prompt: Option<&str>) -> Context {
    Context {
        system_prompt: system_prompt.map(str::to_string),
        messages: vec![Message::User {
            content: UserContent::Text("Hello".to_string()),
            timestamp: 0,
        }],
        tools: None,
    }
}

fn options_with_reasoning(reasoning: ThinkingLevel, region: Option<&str>) -> BedrockOptions {
    BedrockOptions {
        reasoning: Some(reasoning),
        region: region.map(str::to_string),
        ..Default::default()
    }
}

fn additional_fields(body: &Value) -> &Value {
    body.get("additionalModelRequestFields")
        .expect("additionalModelRequestFields")
}

fn opus_4_8_model() -> elph_ai::types::Model {
    let mut model = get_builtin_model("amazon-bedrock", "global.anthropic.claude-opus-4-6-v1").expect("model");
    model.id = "global.anthropic.claude-opus-4-8-v1".to_string();
    model.name = "Claude Opus 4.8 (Global)".to_string();
    model
}

#[test]
fn uses_adaptive_thinking_for_claude_opus_4_8_when_reasoning_is_enabled() {
    let body = build_bedrock_converse_body(
        &opus_4_8_model(),
        &make_context(None),
        &options_with_reasoning(ThinkingLevel::High, None),
    )
    .expect("payload");
    let fields = additional_fields(&body);
    assert_eq!(
        fields["thinking"],
        serde_json::json!({ "type": "adaptive", "display": "summarized" })
    );
    assert_eq!(fields["output_config"], serde_json::json!({ "effort": "high" }));
    assert!(fields.get("anthropic_beta").is_none());
}

#[test]
fn maps_xhigh_reasoning_to_effort_xhigh_for_claude_opus_4_8() {
    let body = build_bedrock_converse_body(
        &opus_4_8_model(),
        &make_context(None),
        &options_with_reasoning(ThinkingLevel::Xhigh, None),
    )
    .expect("payload");
    let fields = additional_fields(&body);
    assert_eq!(
        fields["thinking"],
        serde_json::json!({ "type": "adaptive", "display": "summarized" })
    );
    assert_eq!(fields["output_config"], serde_json::json!({ "effort": "xhigh" }));
    assert!(fields.get("anthropic_beta").is_none());
}

#[test]
fn uses_adaptive_thinking_for_claude_fable_5_when_reasoning_is_enabled() {
    let model = get_builtin_model("amazon-bedrock", "global.anthropic.claude-fable-5").expect("model");
    let body =
        build_bedrock_converse_body(&model, &make_context(None), &options_with_reasoning(ThinkingLevel::High, None))
            .expect("payload");
    let fields = additional_fields(&body);
    assert_eq!(
        fields["thinking"],
        serde_json::json!({ "type": "adaptive", "display": "summarized" })
    );
    assert_eq!(fields["output_config"], serde_json::json!({ "effort": "high" }));
    assert!(fields.get("anthropic_beta").is_none());
}

#[test]
fn uses_adaptive_thinking_for_claude_sonnet_5_when_reasoning_is_enabled() {
    let model = get_builtin_model("amazon-bedrock", "global.anthropic.claude-sonnet-5").expect("model");
    let body =
        build_bedrock_converse_body(&model, &make_context(None), &options_with_reasoning(ThinkingLevel::High, None))
            .expect("payload");
    let fields = additional_fields(&body);
    assert_eq!(
        fields["thinking"],
        serde_json::json!({ "type": "adaptive", "display": "summarized" })
    );
    assert_eq!(fields["output_config"], serde_json::json!({ "effort": "high" }));
    assert!(fields.get("anthropic_beta").is_none());
}

#[test]
fn maps_xhigh_reasoning_to_effort_xhigh_for_claude_fable_5() {
    let model = get_builtin_model("amazon-bedrock", "global.anthropic.claude-fable-5").expect("model");
    let body =
        build_bedrock_converse_body(&model, &make_context(None), &options_with_reasoning(ThinkingLevel::Xhigh, None))
            .expect("payload");
    let fields = additional_fields(&body);
    assert_eq!(
        fields["thinking"],
        serde_json::json!({ "type": "adaptive", "display": "summarized" })
    );
    assert_eq!(fields["output_config"], serde_json::json!({ "effort": "xhigh" }));
}

#[test]
fn omits_display_for_govcloud_model_ids_on_non_adaptive_claude_thinking() {
    let mut model = get_builtin_model("amazon-bedrock", "us.anthropic.claude-sonnet-4-5-20250929-v1:0").expect("model");
    model.id = "us-gov.anthropic.claude-sonnet-4-5-20250929-v1:0".to_string();
    model.name = "Claude Sonnet 4.5 (GovCloud)".to_string();
    let body =
        build_bedrock_converse_body(&model, &make_context(None), &options_with_reasoning(ThinkingLevel::High, None))
            .expect("payload");
    let fields = additional_fields(&body);
    assert_eq!(
        fields["thinking"],
        serde_json::json!({ "type": "enabled", "budget_tokens": 16384 })
    );
    assert_eq!(fields["anthropic_beta"], serde_json::json!(["interleaved-thinking-2025-05-14"]));
}

#[test]
fn omits_display_for_govcloud_regions_on_adaptive_claude_thinking() {
    let body = build_bedrock_converse_body(
        &opus_4_8_model(),
        &make_context(None),
        &options_with_reasoning(ThinkingLevel::High, Some("us-gov-west-1")),
    )
    .expect("payload");
    let fields = additional_fields(&body);
    assert_eq!(fields["thinking"], serde_json::json!({ "type": "adaptive" }));
    assert_eq!(fields["output_config"], serde_json::json!({ "effort": "high" }));
    assert!(fields.get("anthropic_beta").is_none());
}

#[test]
fn uses_adaptive_thinking_when_model_name_contains_model_name_but_arn_does_not() {
    let mut model = get_builtin_model("amazon-bedrock", "global.anthropic.claude-opus-4-6-v1").expect("model");
    model.id = "arn:aws:bedrock:us-east-1:123456789012:application-inference-profile/my-profile".to_string();
    model.name = "Claude Opus 4.6".to_string();
    let body =
        build_bedrock_converse_body(&model, &make_context(None), &options_with_reasoning(ThinkingLevel::High, None))
            .expect("payload");
    let fields = additional_fields(&body);
    assert_eq!(
        fields["thinking"],
        serde_json::json!({ "type": "adaptive", "display": "summarized" })
    );
    assert_eq!(fields["output_config"], serde_json::json!({ "effort": "high" }));
}

#[test]
fn injects_cache_points_when_model_name_identifies_supported_claude_model() {
    let mut model = get_builtin_model("amazon-bedrock", "global.anthropic.claude-opus-4-6-v1").expect("model");
    model.id = "arn:aws:bedrock:us-east-1:123456789012:application-inference-profile/my-profile".to_string();
    model.name = "Claude Sonnet 4.6".to_string();
    let body = build_bedrock_converse_body(&model, &make_context(Some("You are helpful.")), &BedrockOptions::default())
        .expect("payload");
    let system = body.get("system").and_then(|v| v.as_array()).expect("system blocks");
    assert_eq!(system.len(), 2);
    assert!(system[1].get("cachePoint").is_some());
    let messages = body.get("messages").and_then(|v| v.as_array()).expect("messages");
    let last_content = messages
        .last()
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
        .expect("last user content");
    assert!(last_content.last().and_then(|b| b.get("cachePoint")).is_some());
}

#[test]
fn falls_back_to_fixed_budget_thinking_for_non_adaptive_claude_via_model_name() {
    let mut model = get_builtin_model("amazon-bedrock", "us.anthropic.claude-sonnet-4-5-20250929-v1:0").expect("model");
    model.id = "arn:aws:bedrock:us-east-1:123456789012:application-inference-profile/my-profile".to_string();
    model.name = "Claude Sonnet 4.5".to_string();
    let body =
        build_bedrock_converse_body(&model, &make_context(None), &options_with_reasoning(ThinkingLevel::High, None))
            .expect("payload");
    let fields = additional_fields(&body);
    assert_eq!(fields["thinking"]["type"], "enabled");
    assert!(fields["thinking"]["budget_tokens"].as_u64().is_some());
    assert_eq!(fields["anthropic_beta"], serde_json::json!(["interleaved-thinking-2025-05-14"]));
}
