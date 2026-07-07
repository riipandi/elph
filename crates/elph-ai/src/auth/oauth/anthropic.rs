use std::sync::Arc;

use crate::auth::types::{AuthEvent, AuthLoginCallbacks, AuthPrompt, OAuthAuth, OAuthCredential};
use crate::auth::{OAuthLoader, lazy_oauth};

use super::callback::{CallbackResult, parse_authorization_input, start_callback_server};
use super::pkce::generate_pkce;

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CALLBACK_PORT: u16 = 53692;
const CALLBACK_PATH: &str = "/callback";
const REDIRECT_URI: &str = "http://localhost:53692/callback";
const SCOPES: &str =
    "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";

pub fn anthropic_oauth() -> OAuthAuth {
    lazy_oauth("Anthropic (Claude Pro/Max)", anthropic_oauth_loader())
}

pub fn anthropic_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { anthropic_oauth_impl() }))
}

fn anthropic_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "Anthropic (Claude Pro/Max)".to_string(),
        login: Arc::new(|callbacks: Arc<dyn AuthLoginCallbacks>| {
            Box::pin(async move {
                let creds = login_anthropic(&callbacks).await?;
                Ok(oauth_credential(creds))
            })
        }),
        refresh: Arc::new(|credential| {
            Box::pin(async move {
                let creds = refresh_anthropic_token(&credential.refresh).await?;
                Ok(oauth_credential(creds))
            })
        }),
        to_auth: Arc::new(|credential| {
            Box::pin(async move {
                Ok(crate::auth::types::ModelAuth {
                    api_key: Some(credential.access),
                    headers: None,
                    base_url: None,
                })
            })
        }),
    }
}

fn oauth_credential(creds: OAuthTokens) -> OAuthCredential {
    OAuthCredential {
        kind: "oauth".to_string(),
        access: creds.access,
        refresh: creds.refresh,
        expires: creds.expires,
        account_id: None,
        enterprise_url: None,
        available_model_ids: None,
    }
}

pub struct OAuthTokens {
    access: String,
    refresh: String,
    expires: i64,
}

pub async fn login_anthropic(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<OAuthTokens> {
    let (verifier, challenge) = generate_pkce().await;
    let server = start_callback_server(
        CALLBACK_PORT,
        CALLBACK_PATH,
        Some(&verifier),
        "Anthropic authentication completed",
    )
    .await?;

    let auth_url = format!(
        "{AUTHORIZE_URL}?code=true&client_id={CLIENT_ID}&response_type=code&redirect_uri={}&scope={}&code_challenge={challenge}&code_challenge_method=S256&state={verifier}",
        urlencoding::encode(REDIRECT_URI),
        urlencoding::encode(SCOPES),
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
                .prompt(AuthPrompt::ManualCode {
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
                    code.map(|c| CallbackResult {
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
    callbacks.notify(AuthEvent::Progress {
        message: "Exchanging authorization code for tokens...".to_string(),
    });
    exchange_authorization_code(&code, &state, &verifier, REDIRECT_URI).await
}

pub async fn refresh_anthropic_token(refresh_token: &str) -> anyhow::Result<OAuthTokens> {
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": CLIENT_ID,
        "refresh_token": refresh_token,
    });
    let response = post_json(TOKEN_URL, &body).await?;
    parse_token_response(&response)
}

async fn exchange_authorization_code(
    code: &str,
    state: &str,
    verifier: &str,
    redirect_uri: &str,
) -> anyhow::Result<OAuthTokens> {
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "client_id": CLIENT_ID,
        "code": code,
        "state": state,
        "redirect_uri": redirect_uri,
        "code_verifier": verifier,
    });
    let response = post_json(TOKEN_URL, &body).await?;
    parse_token_response(&response)
}

async fn post_json(url: &str, body: &serde_json::Value) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow::anyhow!("HTTP {status} from {url}: {text}"));
    }
    Ok(text)
}

fn parse_token_response(body: &str) -> anyhow::Result<OAuthTokens> {
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
    Ok(OAuthTokens {
        access: access.to_string(),
        refresh: refresh.to_string(),
        expires: chrono::Utc::now().timestamp_millis() + (expires_in as i64 * 1000) - 5 * 60 * 1000,
    })
}

// urlencoding helper - avoid extra dep
mod urlencoding {
    pub fn encode(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }
}
