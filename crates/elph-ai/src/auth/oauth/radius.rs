//! Radius OAuth PKCE flow for Inflection AI (pi-messages gateway).
//!
//! Ported from pi-ai `auth/oauth/radius.ts`.

use std::sync::Arc;

use crate::auth::OAuthLoader;
use crate::auth::types::{AuthEvent, AuthLoginCallbacks, OAuthAuth, OAuthCredential};

use super::callback::{parse_authorization_input, start_callback_server};
use super::pkce::generate_pkce;

const RADIUS_CLIENT_ID: &str = "pi-messages";
const RADIUS_AUTHORIZE_URL: &str = "https://oauth.pi.ai/authorize";
const RADIUS_TOKEN_URL: &str = "https://oauth.pi.ai/token";
const CALLBACK_PORT: u16 = 53692;
const CALLBACK_PATH: &str = "/callback";
const REDIRECT_URI: &str = "http://localhost:53692/callback";
const SCOPES: &str = "openid profile email";

pub fn radius_oauth() -> OAuthAuth {
    radius_oauth_impl()
}

pub fn radius_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { radius_oauth_impl() }))
}

fn radius_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "Inflection AI (pi-messages)".to_string(),
        login: Arc::new(|callbacks: Arc<dyn AuthLoginCallbacks>| {
            Box::pin(async move {
                let creds = login_radius(&callbacks).await?;
                Ok(creds)
            })
        }),
        refresh: Arc::new(|credential| Box::pin(async move { refresh_radius_token(&credential.refresh).await })),
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

async fn login_radius(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<OAuthCredential> {
    let (verifier, challenge) = generate_pkce().await;
    let server =
        start_callback_server(CALLBACK_PORT, CALLBACK_PATH, Some(&verifier), "Radius authentication completed").await?;

    let auth_url = format!(
        "{RADIUS_AUTHORIZE_URL}?response_type=code&client_id={RADIUS_CLIENT_ID}&redirect_uri={REDIRECT_URI}&scope={SCOPES}&code_challenge={challenge}&code_challenge_method=S256&state={verifier}",
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

    exchange_authorization_code(&code, &state, &verifier, REDIRECT_URI).await
}

async fn exchange_authorization_code(
    code: &str,
    state: &str,
    verifier: &str,
    redirect_uri: &str,
) -> anyhow::Result<OAuthCredential> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "client_id": RADIUS_CLIENT_ID,
        "code": code,
        "state": state,
        "redirect_uri": redirect_uri,
        "code_verifier": verifier,
    });
    let response = client
        .post(RADIUS_TOKEN_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow::anyhow!("HTTP {status} from Radius token exchange: {text}"));
    }
    parse_token_response(&text)
}

async fn refresh_radius_token(refresh_token: &str) -> anyhow::Result<OAuthCredential> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": RADIUS_CLIENT_ID,
        "refresh_token": refresh_token,
    });
    let response = client
        .post(RADIUS_TOKEN_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow::anyhow!("HTTP {status} from Radius token refresh: {text}"));
    }
    parse_token_response(&text)
}

fn parse_token_response(body: &str) -> anyhow::Result<OAuthCredential> {
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
    Ok(oauth_credential(
        access.to_string(),
        refresh.to_string(),
        chrono::Utc::now().timestamp_millis() + (expires_in as i64 * 1000) - 5 * 60 * 1000,
    ))
}
