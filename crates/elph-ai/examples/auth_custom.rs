//! Custom auth: `AuthContext`, `CredentialStore`, `env_api_key_auth`.
//!
//! Building blocks behind automatic auth resolution: custom env lookup,
//! credential storage, and API-key auth builder — then runs a real
//! completion against opencode/big-pickle.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-ai --example auth_custom
//! ```

use std::sync::Arc;

use elph_ai::{
    AuthContext, BoxFuture, Context, CreateModelsOptions, InMemoryCredentialStore, Message, UserContent,
    env_api_key_auth,
};

// ── Custom auth context: try CUSTOM_<KEY> first, then fallback ──
struct PrefixedEnv;

impl AuthContext for PrefixedEnv {
    fn env<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Option<String>> {
        let name = name.to_string();
        Box::pin(async move {
            let prefixed = format!("CUSTOM_{name}");
            std::env::var(&prefixed).ok().or_else(|| std::env::var(&name).ok())
        })
    }

    fn file_exists<'a>(&'a self, _path: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(async { false })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Models with custom auth context (builtin providers included) ──
    let models = elph_ai::builtin_models(Some(CreateModelsOptions {
        credentials: Some(Arc::new(InMemoryCredentialStore::new())),
        auth_context: Some(Arc::new(PrefixedEnv)),
    }));

    // ── env_api_key_auth builder ──
    let key_auth = env_api_key_auth("OpenCode Key", vec!["CUSTOM_OPENCODE_API_KEY", "OPENCODE_API_KEY"]);
    println!("Auth name: {}", key_auth.name);

    // ── Lookup model ──
    let model = match models.get_model("opencode", "big-pickle") {
        Some(m) => m,
        None => anyhow::bail!("opencode/big-pickle not found"),
    };

    let auth = models.get_auth(&model).await?;
    match &auth {
        Some(a) => println!("Auth source: {}", a.source.as_deref().unwrap_or("store")),
        None => println!("No auth — set OPENCODE_API_KEY"),
    }

    // ── Custom env resolution in action ──
    let env_ctx = PrefixedEnv;
    let home = env_ctx.env("HOME").await.unwrap_or_else(|| "?".into());
    println!("HOME via PrefixedEnv: {home}");

    let context = Context {
        system_prompt: Some("Reply in one sentence.".into()),
        messages: vec![Message::User {
            content: UserContent::Text("What is a lifetime in Rust?".into()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }],
        tools: None,
    };

    let message = models.complete(&model, &context, None).await;
    match &message.error_message {
        Some(e) => println!("Chat unavailable: {e}"),
        None => {
            for block in &message.content {
                if let elph_ai::AssistantContentBlock::Text(t) = block {
                    println!("Answer: {}", t.text);
                }
            }
        }
    }

    Ok(())
}
