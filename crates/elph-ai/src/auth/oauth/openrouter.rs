//! OpenRouter OAuth PKCE flow.
//!
//! Ported from pi-ai `auth/oauth/openrouter.ts`.
//! PKCE login that mints a user-controlled API key.

use std::sync::Arc;

use crate::auth::OAuthLoader;
use crate::auth::types::{AuthEvent, AuthLoginCallbacks, OAuthAuth, OAuthCredential};

use super::callback::{parse_authorization_input, start_callback_server};
use super::pkce::generate_pkce;

const OPENROUTER_CLIENT_ID: &str = "openrouter";
const OPENROUTER_AUTHORIZE_URL: &str = "https://openrouter.ai/auth/authorize";
const OPENROUTER_TOKEN_URL: &str = "https://openrouter.ai/api/v1/auth/token";
const OPENROUTER_KEYS_URL: &str = "https://openrouter.ai/api/v1/auth/keys";
const CALLBACK_PORT: u16 = 53693;
const CALLBACK_PATH: &str = "/callback";
const REDIRECT_URI: &str = "http://localhost:53693/callback";
const SCOPES: &str = "openid profile email";

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
        refresh: Arc::new(|credential| Box::pin(async move { refresh_openrouter_token(&credential.refresh).await })),
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

fn oauth_credential(access: String, refresh: String, expires: i64) -> OAuthCredential {
    OAuthCredential {
        kind: "oauth".to_string(),
        access,
        refresh,
        expires,
        account_id: None,
        enterprise_url: None,
        available_model_ids: None,
    }
}

async fn mint_api_key(access_token: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .post(OPENROUTER_KEYS_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "name": "elph",
            "limit": null,
        }))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to mint OpenRouter API key: {e}"))?;

    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Invalid response: {e}"))?;

    if !status.is_success() {
        let error = body.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
        anyhow::bail!("OpenRouter API key minting failed: {error}");
    }

    body.get("key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Missing key in OpenRouter response"))
}

async fn login_openrouter(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<OAuthCredential> {
    let (verifier, challenge) = generate_pkce().await;
    let server = start_callback_server(
        CALLBACK_PORT,
        CALLBACK_PATH,
        Some(&verifier),
        "OpenRouter authentication completed",
    )
    .await?;

    let auth_url = format!(
        "{OPENROUTER_AUTHORIZE_URL}?response_type=code&client_id={OPENROUTER_CLIENT_ID}&redirect_uri={REDIRECT_URI}&scope={SCOPES}&code_challenge={challenge}&code_challenge_method=S256&state={verifier}",
    );
    callbacks.notify(AuthEvent::AuthUrl {
        url: auth_url,
        instructions: Some(
            "Complete login in your browser. If the browser is on another machine, paste the final redirect URL here."
                .to_string(),
        ),
    });

    let callbacks_for_manual = callbacks.clone();
    let verifier_for_manual = verifier.clone();
    let callback = tokio::select! {
        result = server.wait_for_code(std::time::Duration::from_secs(600)) => result,
        input = async move {
            callbacks_for_manual
                .prompt(crate::auth::types::AuthPrompt::ManualCode {
                    message: "Complete login in your browser, or paste the authorization code / redirect URL here:".to_string(),
                    placeholder: Some(REDIRECT_URI.to_string()),
                })
                .await
                .ok()
                .and_then(|input| {
                    let (code, state) = parse_authorization_input(&input);
                    if let Some(ref s) = state
                        && s != &verifier_for_manual {
                            return None;
                        }
                    code.map(|c| super::callback::CallbackResult {
                        code: c,
                        state: state.or(Some(verifier_for_manual.clone())),
                    })
                })
        } => input,
    };

    let (code, state) = if let Some(result) = callback {
        (Some(result.code), result.state)
    } else {
        (None, None)
    };

    let code = code.ok_or_else(|| anyhow::anyhow!("Missing authorization code"))?;
    let state = state.ok_or_else(|| anyhow::anyhow!("Missing OAuth state"))?;

    callbacks.notify(crate::auth::types::AuthEvent::Progress {
        message: "Exchanging authorization code for tokens...".to_string(),
    });

    let tokens = exchange_authorization_code(&code, &state, &verifier, REDIRECT_URI).await?;

    // Mint a user-controlled API key
    callbacks.notify(crate::auth::types::AuthEvent::Progress {
        message: "Minting API key...".to_string(),
    });
    let api_key = mint_api_key(&tokens.access).await?;

    Ok(oauth_credential(api_key, tokens.refresh, tokens.expires))
}

async fn exchange_authorization_code(
    code: &str,
    state: &str,
    verifier: &str,
    redirect_uri: &str,
) -> anyhow::Result<super::anthropic::OAuthTokens> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "client_id": OPENROUTER_CLIENT_ID,
        "code": code,
        "state": state,
        "redirect_uri": redirect_uri,
        "code_verifier": verifier,
    });
    let response = client
        .post(OPENROUTER_TOKEN_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow::anyhow!("HTTP {status} from OpenRouter token exchange: {text}"));
    }
    parse_token_response(&text)
}

async fn refresh_openrouter_token(refresh_token: &str) -> anyhow::Result<OAuthCredential> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": OPENROUTER_CLIENT_ID,
        "refresh_token": refresh_token,
    });
    let response = client
        .post(OPENROUTER_TOKEN_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow::anyhow!("HTTP {status} from OpenRouter token refresh: {text}"));
    }
    let tokens = parse_token_response(&text)?;
    Ok(oauth_credential(tokens.access, tokens.refresh, tokens.expires))
}

fn parse_token_response(body: &str) -> anyhow::Result<super::anthropic::OAuthTokens> {
    let data: serde_json::Value = serde_json::from_str(body)?;
    let access = data
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing access_token"))?;
    let refresh = data
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing refresh_token"))?;
    let expires_in = data
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("missing expires_in"))?;
    Ok(super::anthropic::OAuthTokens {
        access: access.to_string(),
        refresh: refresh.to_string(),
        expires: chrono::Utc::now().timestamp_millis() + (expires_in as i64 * 1000) - 5 * 60 * 1000,
    })
}
