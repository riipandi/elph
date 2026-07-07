//! Live OpenRouter cache_write regression test for elph-ai.
//! Run with: `cargo test -p elph-ai --test openrouter_cache_write_live -- --ignored`

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

use elph_ai::api::common::wrap_on_payload;
use elph_ai::types::{Message, Model, SimpleStreamOptions, StreamOptions, UserContent};
use elph_ai::{builtin_models, get_builtin_model};
use serde_json::{Value, json};

static NONCE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn has_env(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty())
}

fn create_long_system_prompt() -> String {
    let nonce = NONCE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let repeated = (0..80)
        .map(|_| {
            "Prompt-caching probe content. Keep this exact text stable across requests so the provider can reuse prefix tokens and report cache read and cache write usage."
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("You are a concise assistant.\nCache nonce: {nonce}\n\n{repeated}")
}

fn add_cache_control_to_last_user_message(payload: Value) -> Value {
    let mut payload = payload;
    let Some(messages) = payload.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return payload;
    };
    for msg in messages.iter_mut().rev() {
        if msg.get("role").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }
        if let Some(content) = msg.get_mut("content") {
            if content.is_string() {
                let text = content.as_str().unwrap_or_default().to_string();
                *content = json!([{
                    "type": "text",
                    "text": text,
                    "cache_control": { "type": "ephemeral" }
                }]);
                break;
            }
            if let Some(parts) = content.as_array_mut() {
                for part in parts.iter_mut().rev() {
                    if part.get("type").and_then(|v| v.as_str()) == Some("text") {
                        part["cache_control"] = json!({ "type": "ephemeral" });
                        break;
                    }
                }
                break;
            }
        }
        break;
    }
    payload
}

#[tokio::test]
#[ignore = "requires OPENROUTER_API_KEY"]
async fn preserves_cache_write_tokens_on_openai_completions_stream_path() {
    assert!(has_env("OPENROUTER_API_KEY"));
    let models = builtin_models(None);
    let model = get_builtin_model("openrouter", "google/gemini-2.5-flash").expect("model");
    let context = elph_ai::types::Context {
        system_prompt: Some(create_long_system_prompt()),
        messages: vec![Message::User {
            content: UserContent::Text("Reply with exactly: OK".to_string()),
            timestamp: 0,
        }],
        tools: None,
    };
    let on_payload = wrap_on_payload(|payload, _model: Model| {
        Box::pin(async move { Some(add_cache_control_to_last_user_message(payload)) })
            as Pin<Box<dyn std::future::Future<Output = Option<Value>> + Send>>
    });
    let options = SimpleStreamOptions {
        base: StreamOptions {
            api_key: std::env::var("OPENROUTER_API_KEY").ok(),
            max_tokens: Some(32),
            temperature: Some(0.0),
            on_payload: Some(on_payload),
            ..Default::default()
        },
        reasoning: None,
        thinking_budgets: None,
    };

    let first = models.complete_simple(&model, &context, Some(options.clone())).await;
    assert_eq!(
        first.stop_reason,
        elph_ai::types::StopReason::Stop,
        "{}",
        first.error_message.unwrap_or_default()
    );

    let second = models.complete_simple(&model, &context, Some(options)).await;
    assert_eq!(
        second.stop_reason,
        elph_ai::types::StopReason::Stop,
        "{}",
        second.error_message.unwrap_or_default()
    );

    let has_cache_write = first.usage.cache_write > 0 || second.usage.cache_write > 0;
    assert!(has_cache_write, "expected cache_write > 0 on at least one call");
}
