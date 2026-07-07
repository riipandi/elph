use elph_ai::api::openai_compat::get_compat;
use elph_ai::api::openai_completions::convert_messages;
use elph_ai::get_builtin_model;
use elph_ai::types::{AssistantContentBlock, AssistantMessage, Context, Message, StopReason, TextContent};
use serde_json::json;

#[test]
fn developer_role_for_reasoning_models() {
    let mut openai = get_builtin_model("openai", "o3-mini").expect("model exists");
    openai.reasoning = true;
    let compat = get_compat(&openai);
    let messages = convert_messages(
        &openai,
        &Context {
            system_prompt: Some("system".to_string()),
            messages: vec![],
            tools: None,
        },
        &compat,
    );
    assert_eq!(messages[0].get("role").and_then(|v| v.as_str()), Some("developer"));
}

#[test]
fn reasoning_content_on_assistant_messages_when_required() {
    let model = get_builtin_model("deepseek", "deepseek-v4-flash").expect("model exists");
    let compat = get_compat(&model);
    let context = Context {
        system_prompt: Some("system".to_string()),
        messages: vec![Message::Assistant(AssistantMessage {
            role: "assistant".to_string(),
            content: vec![AssistantContentBlock::Text(TextContent::new("hi"))],
            api: model.api.clone(),
            provider: model.provider.clone(),
            model: model.id.clone(),
            usage: Default::default(),
            stop_reason: StopReason::Stop,
            timestamp: 0,
            response_id: None,
            response_model: None,
            error_message: None,
        })],
        tools: None,
    };
    let messages = convert_messages(&model, &context, &compat);
    assert_eq!(
        messages
            .iter()
            .find(|m| m.get("role") == Some(&json!("assistant")))
            .and_then(|m| m.get("reasoning_content"))
            .and_then(|v| v.as_str()),
        Some("")
    );
}
