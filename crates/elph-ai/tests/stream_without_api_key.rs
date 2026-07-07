use elph_ai::types::{Context, Message, UserContent};
use elph_ai::{StopReason, builtin_models, get_builtin_model};

#[tokio::test]
async fn stream_without_api_key_returns_error_message() {
    let models = builtin_models(None);
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model exists");
    let context = Context {
        system_prompt: Some("You are helpful.".to_string()),
        messages: vec![Message::User {
            content: UserContent::Text("Hello".to_string()),
            timestamp: 0,
        }],
        tools: None,
    };
    let response = models.complete(&model, &context, None).await;
    assert!(
        response.error_message.is_some() || response.stop_reason == StopReason::Error,
        "expected auth/network error without API key"
    );
}
