//! Live abort tests for elph-ai.
//! Run with: `cargo test -p elph-ai --test abort_live -- --ignored`

mod common;

use common::sample_user_context;
use elph_ai::types::{SimpleStreamOptions, StopReason, StreamOptions, ThinkingLevel};
use elph_ai::{builtin_models, get_builtin_model};
use tokio_util::sync::CancellationToken;

fn has_env(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty())
}

fn default_simple_options() -> SimpleStreamOptions {
    SimpleStreamOptions {
        base: StreamOptions::default(),
        reasoning: None,
        thinking_budgets: None,
    }
}

async fn test_immediate_abort(model: &elph_ai::types::Model, options: Option<SimpleStreamOptions>) {
    let token = CancellationToken::new();
    token.cancel();
    let models = builtin_models(None);
    let mut opts = options.unwrap_or_else(default_simple_options);
    opts.base.signal = Some(token);
    let response = models
        .complete_simple(model, &sample_user_context(Some("You are helpful.")), Some(opts))
        .await;
    assert_eq!(response.stop_reason, StopReason::Aborted);
}

async fn test_abort_then_new_message(model: &elph_ai::types::Model, options: Option<SimpleStreamOptions>) {
    let models = builtin_models(None);
    let token = CancellationToken::new();
    token.cancel();
    let mut abort_opts = options.clone().unwrap_or_else(default_simple_options);
    abort_opts.base.signal = Some(token);

    let mut context = sample_user_context(Some("You are helpful."));
    let aborted = models.complete_simple(model, &context, Some(abort_opts)).await;
    assert_eq!(aborted.stop_reason, StopReason::Aborted);
    assert!(aborted.content.is_empty());

    context.messages.push(elph_ai::types::Message::Assistant(aborted));
    context.messages.push(elph_ai::types::Message::User {
        content: elph_ai::types::UserContent::Text("What is 2 + 2?".to_string()),
        timestamp: 1,
    });

    let follow_up = models.complete_simple(model, &context, options).await;
    assert_eq!(follow_up.stop_reason, StopReason::Stop);
    assert!(!follow_up.content.is_empty());
}

#[tokio::test]
#[ignore = "requires GEMINI_API_KEY"]
async fn google_handles_immediate_abort() {
    assert!(has_env("GEMINI_API_KEY"));
    let model = get_builtin_model("google", "gemini-2.5-flash").expect("model");
    test_immediate_abort(
        &model,
        Some(SimpleStreamOptions {
            base: StreamOptions::default(),
            reasoning: Some(ThinkingLevel::Low),
            thinking_budgets: None,
        }),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_responses_handles_immediate_abort() {
    assert!(has_env("OPENAI_API_KEY"));
    let model = get_builtin_model("openai", "gpt-5-mini").expect("model");
    test_immediate_abort(&model, None).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_completions_handles_immediate_abort() {
    assert!(has_env("OPENAI_API_KEY"));
    let mut model = get_builtin_model("openai", "gpt-4o-mini").expect("model");
    model.api = "openai-completions".to_string();
    test_immediate_abort(&model, None).await;
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_handles_immediate_abort() {
    assert!(has_env("ANTHROPIC_API_KEY"));
    let model = get_builtin_model("anthropic", "claude-haiku-4-5").expect("model");
    test_immediate_abort(
        &model,
        Some(SimpleStreamOptions {
            base: StreamOptions::default(),
            reasoning: Some(ThinkingLevel::Low),
            thinking_budgets: None,
        }),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires MISTRAL_API_KEY"]
async fn mistral_handles_immediate_abort() {
    assert!(has_env("MISTRAL_API_KEY"));
    let model = get_builtin_model("mistral", "devstral-medium-latest").expect("model");
    test_immediate_abort(&model, None).await;
}

#[tokio::test]
#[ignore = "requires AWS_BEARER_TOKEN_BEDROCK or AWS credentials"]
async fn bedrock_handles_immediate_abort() {
    let has_bearer = has_env("AWS_BEARER_TOKEN_BEDROCK");
    let has_aws = has_env("AWS_ACCESS_KEY_ID") || has_env("AWS_PROFILE");
    assert!(has_bearer || has_aws, "bedrock credentials required");
    let model = get_builtin_model("amazon-bedrock", "global.anthropic.claude-sonnet-4-5-20250929-v1:0").expect("model");
    test_immediate_abort(
        &model,
        Some(SimpleStreamOptions {
            base: StreamOptions::default(),
            reasoning: Some(ThinkingLevel::Medium),
            thinking_budgets: None,
        }),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires AWS_BEARER_TOKEN_BEDROCK or AWS credentials"]
async fn bedrock_handles_abort_then_new_message() {
    let has_bearer = has_env("AWS_BEARER_TOKEN_BEDROCK");
    let has_aws = has_env("AWS_ACCESS_KEY_ID") || has_env("AWS_PROFILE");
    assert!(has_bearer || has_aws, "bedrock credentials required");
    let model = get_builtin_model("amazon-bedrock", "global.anthropic.claude-sonnet-4-5-20250929-v1:0").expect("model");
    test_abort_then_new_message(&model, None).await;
}
