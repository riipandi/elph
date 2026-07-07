//! Live provider tests mirroring pi-ai `skipIf` suites.
//! Run with: `cargo test -p elph-ai --test e2e_live -- --ignored`

mod common;

use common::sample_user_context;
use elph_ai::{builtin_models, get_builtin_model};

fn has_env(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty())
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_stream_returns_assistant_content() {
    assert!(has_env("OPENAI_API_KEY"));
    let models = builtin_models(None);
    let model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    let response = models
        .complete(&model, &sample_user_context(Some("You are helpful.")), None)
        .await;
    assert!(response.error_message.is_none());
    assert!(!response.content.is_empty());
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_stream_returns_assistant_content() {
    assert!(has_env("ANTHROPIC_API_KEY"));
    let models = builtin_models(None);
    let model = get_builtin_model("anthropic", "claude-haiku-4-5").expect("model");
    let response = models
        .complete(&model, &sample_user_context(Some("You are helpful.")), None)
        .await;
    assert!(response.error_message.is_none());
    assert!(!response.content.is_empty());
}

#[tokio::test]
#[ignore = "requires GEMINI_API_KEY"]
async fn google_stream_returns_assistant_content() {
    assert!(has_env("GEMINI_API_KEY"));
    let models = builtin_models(None);
    let model = get_builtin_model("google", "gemini-2.5-flash").expect("model");
    let response = models
        .complete(&model, &sample_user_context(Some("You are helpful.")), None)
        .await;
    assert!(response.error_message.is_none());
    assert!(!response.content.is_empty());
}
