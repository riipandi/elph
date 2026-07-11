//! Register a custom OpenAI-compatible provider at runtime.
//!
//! Defines models, auth, and API binding via `create_provider`, then
//! registers with `MutableModels::set_provider`.
//!
//! ```bash
//! export CUSTOM_API_KEY="sk-..."  # optional — works with opencode too
//! cargo run -p elph-ai --example custom_provider
//! ```

use std::sync::Arc;

use elph_ai::{
    ApiKeyAuth, AuthResolveInput, AuthResult, Context, Message, Model, ModelAuth, ModelCost, MutableModels,
    ProviderApi, ProviderAuth, UserContent, create_models, create_provider,
};

// Real model reachable without a key (opencode big-pickle)
const REAL_MODEL: &str = "big-pickle";
const REAL_BASE_URL: &str = "https://opencode.ai/zen/v1";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── 1. Build a custom model ──
    let custom_model = Model {
        id: REAL_MODEL.to_string(),
        name: "Big Pickle (via custom provider)".to_string(),
        api: "openai-completions".to_string(),
        provider: "my-provider".to_string(),
        base_url: REAL_BASE_URL.to_string(),
        reasoning: false,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: ModelCost {
            input: 0.15,
            output: 0.60,
            cache_read: 0.075,
            cache_write: 0.15,

            tiers: None,
        },
        context_window: 128_000,
        max_tokens: 16_384,
        headers: None,
        openai_completions_compat: None,
        openai_responses_compat: None,
        anthropic_compat: None,
    };

    // ── 2. Build custom provider ──
    let provider = create_provider(elph_ai::CreateProviderOptions {
        id: "my-provider".to_string(),
        name: Some("My Custom Provider".to_string()),
        base_url: Some(REAL_BASE_URL.to_string()),
        headers: None,
        auth: ProviderAuth {
            api_key: Some(ApiKeyAuth {
                name: "API Key".to_string(),
                resolve: Arc::new(|_input: AuthResolveInput| {
                    Box::pin(async {
                        let key = std::env::var("CUSTOM_API_KEY")
                            .ok()
                            .or_else(|| std::env::var("OPENCODE_API_KEY").ok())
                            .filter(|k| !k.is_empty());
                        key.map(|k| AuthResult {
                            auth: ModelAuth {
                                api_key: Some(k),
                                headers: None,
                                base_url: None,
                            },
                            env: None,
                            source: Some("CUSTOM_API_KEY / OPENCODE_API_KEY".to_string()),
                        })
                    })
                }),
                login: None,
            }),
            oauth: None,
        },
        models: vec![custom_model],
        refresh_models: None,
        api: ProviderApi::Single(elph_ai::providers::adapter::openai_completions_api()),
    });

    // ── 3. Register with Models ──
    let mut models: MutableModels = create_models(None);
    models.set_provider(provider);

    // ── 4. Use ──
    let model = models.get_model("my-provider", REAL_MODEL).expect("model registered");

    let context = Context {
        system_prompt: Some("Answer concisely.".into()),
        messages: vec![Message::User {
            content: UserContent::Text("What is Rust's borrow checker?".into()),
            timestamp: timestamp(),
        }],
        tools: None,
    };

    let message = models.complete(&model, &context, None).await;

    if let Some(err) = &message.error_message {
        println!("Error: {err}");
        println!();
        println!("Tip: opencode big-pickle is available without an API key.");
        println!("Set OPENCODE_API_KEY if you hit rate limits.");
        return Ok(());
    }

    print!("Answer: ");
    for block in &message.content {
        if let elph_ai::AssistantContentBlock::Text(t) = block {
            print!("{}", t.text);
        }
    }
    println!();
    println!("Stop:    {:?}", message.stop_reason);
    println!("Tokens:  {}", message.usage.total_tokens);

    Ok(())
}

fn timestamp() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
