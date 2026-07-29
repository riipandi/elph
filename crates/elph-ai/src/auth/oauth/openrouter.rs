//! OpenRouter OAuth PKCE flow.
//!
//! Ported from pi-ai `auth/oauth/openrouter.ts`.
//! PKCE login that mints a user-controlled API key directly via a single
//! token exchange at the keys endpoint — no separate OAuth2 token grant.

use std::sync::Arc;
use std::time::Duration;

use crate::auth::OAuthLoader;
use crate::auth::types::{AuthEvent, AuthLoginCallbacks, AuthPrompt, OAuthAuth, OAuthCredential};

use super::callback::{parse_authorization_input, start_callback_server};
use super::pkce::generate_pkce;

const OPENROUTER_AUTHORIZE_URL: &str = "https://openrouter.ai/auth";
const OPENROUTER_KEYS_URL: &str = "https://openrouter.ai/api/v1/auth/keys";
const CALLBACK_PORT: u16 = 53693;
const CALLBACK_PATH: &str = "/callback";
const REDIRECT_URI: &str = "http://localhost:53693/callback";

pub fn openrouter_oauth() -> OAuthAuth {
    openrouter_oauth_impl()
}

pub fn openrouter_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { openrouter_oauth_impl() }))
}

fn openrouter_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "OpenRouter".to_string(),
        login: Arc::new(|callbacks: Arc<dyn AuthLoginCallbacks>| {
            Box::pin(async move {
                let creds = login_openrouter(&callbacks).await?;
                Ok(creds)
            })
        }),
        // OpenRouter API keys do not expire — refresh is a no-op.
        refresh: Arc::new(|credential| Box::pin(async move { Ok(credential) })),
        to_auth: Arc::new(|credential| {
            Box::pin(async move {
                Ok(crate::auth::types::ModelAuth {
                    api_key: Some(credential.access.clone()),
                    headers: None,
                    base_url: None,
                })
            })
        }),
    }
}

/// Exchange an authorization code + verifier for a permanent API key.
async fn exchange_code_for_key(code: &str, verifier: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .post(OPENROUTER_KEYS_URL)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "code": code,
            "code_verifier": verifier,
            "code_challenge_method": "S256",
        }))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("OpenRouter key exchange request failed: {e}"))?;

    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Invalid JSON from OpenRouter: {e}"))?;

    if !status.is_success() {
        let detail = body
            .get("error_description")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("message").and_then(|v| v.as_str()))
            .or_else(|| body.get("error").and_then(|v| v.as_str()))
            .unwrap_or("unknown");
        anyhow::bail!("OpenRouter key exchange failed (HTTP {status}): {detail}");
    }

    body.get("key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("OpenRouter response missing API key"))
}

async fn login_openrouter(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<OAuthCredential> {
    let (verifier, challenge) = generate_pkce().await;
    let server = start_callback_server(
        CALLBACK_PORT,
        CALLBACK_PATH,
        None,
        "Signed in to OpenRouter. You may now close this page.",
    )
    .await?;

    let auth_url = format!(
        "{OPENROUTER_AUTHORIZE_URL}?callback_url={REDIRECT_URI}&code_challenge={challenge}&code_challenge_method=S256",
    );
    callbacks.notify(AuthEvent::AuthUrl {
        url: auth_url,
        instructions: Some(
            "Complete sign-in in your browser. If the browser is on another machine, paste the final redirect URL here."
                .to_string(),
        ),
    });

    let callbacks_for_manual = callbacks.clone();
    let callback = tokio::select! {
        result = server.wait_for_code(std::time::Duration::from_secs(600)) => result,
        input = async move {
            callbacks_for_manual
                .prompt(AuthPrompt::ManualCode {
                    message: "Complete sign-in in your browser, or paste the authorization code / redirect URL here:".to_string(),
                    placeholder: Some(REDIRECT_URI.to_string()),
                })
                .await
                .ok()
                .and_then(|input| {
                    parse_authorization_input(&input).0.map(|code| super::callback::CallbackResult {
                        code,
                        state: None,
                    })
                })
        } => input,
    };

    let code = callback
        .and_then(|r| {
            if r.code.is_empty() { None } else { Some(r.code) }
        })
        .ok_or_else(|| anyhow::anyhow!("Missing authorization code"))?;

    callbacks.notify(crate::auth::types::AuthEvent::Progress {
        message: "Exchanging authorization code for an API key...".to_string(),
    });

    let api_key = exchange_code_for_key(&code, &verifier).await?;

    Ok(OAuthCredential {
        kind: "oauth".to_string(),
        access: api_key,
        refresh: String::new(),
        expires: i64::MAX,
        account_id: None,
        enterprise_url: None,
        available_model_ids: None,
    })
}
