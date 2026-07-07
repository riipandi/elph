use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use rand::Rng;
use serde_json::Value;

use crate::auth::types::{AuthEvent, AuthLoginCallbacks, AuthPrompt, OAuthAuth, OAuthCredential};
use crate::auth::{OAuthLoader, lazy_oauth};

use super::callback::{parse_authorization_input, start_callback_server};
use super::device_code::{DeviceCodePollOptions, DeviceCodePollResult, poll_oauth_device_code_flow};
use super::pkce::generate_pkce;

pub const OPENAI_CODEX_BROWSER_LOGIN_METHOD: &str = "browser";
pub const OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD: &str = "device_code";

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const DEVICE_USER_CODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const DEVICE_VERIFICATION_URI: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
const DEVICE_CODE_TIMEOUT_SECONDS: u64 = 15 * 60;
const SCOPE: &str = "openid profile email offline_access";
const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";

pub fn openai_codex_oauth() -> OAuthAuth {
    lazy_oauth("OpenAI (ChatGPT Plus/Pro)", openai_codex_oauth_loader())
}

pub fn openai_codex_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { openai_codex_oauth_impl() }))
}

fn openai_codex_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "OpenAI (ChatGPT Plus/Pro)".to_string(),
        login: Arc::new(|callbacks: Arc<dyn AuthLoginCallbacks>| {
            Box::pin(async move {
                let method = callbacks
                    .prompt(AuthPrompt::Select {
                        message: "Select OpenAI Codex login method:".to_string(),
                        options: vec![
                            crate::auth::types::AuthSelectOption {
                                id: OPENAI_CODEX_BROWSER_LOGIN_METHOD.to_string(),
                                label: "Browser login (default)".to_string(),
                                description: None,
                            },
                            crate::auth::types::AuthSelectOption {
                                id: OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD.to_string(),
                                label: "Device code login (headless)".to_string(),
                                description: None,
                            },
                        ],
                    })
                    .await?;

                let creds = if method == OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD {
                    login_openai_codex_device_code(callbacks).await?
                } else if method == OPENAI_CODEX_BROWSER_LOGIN_METHOD {
                    login_openai_codex(callbacks).await?
                } else {
                    return Err(anyhow::anyhow!("Unknown OpenAI Codex login method: {method}"));
                };
                Ok(to_oauth_credential(creds))
            })
        }),
        refresh: Arc::new(|credential| {
            Box::pin(async move {
                let creds = refresh_openai_codex_token(&credential.refresh).await?;
                Ok(to_oauth_credential(creds))
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

pub struct CodexOAuthTokens {
    access: String,
    refresh: String,
    expires: i64,
    account_id: String,
}

fn to_oauth_credential(creds: CodexOAuthTokens) -> OAuthCredential {
    OAuthCredential {
        kind: "oauth".to_string(),
        access: creds.access,
        refresh: creds.refresh,
        expires: creds.expires,
        account_id: Some(creds.account_id),
        enterprise_url: None,
        available_model_ids: None,
    }
}

pub async fn login_openai_codex(callbacks: Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<CodexOAuthTokens> {
    let (verifier, challenge) = generate_pkce().await;
    let state = create_state();
    let auth_url = build_authorize_url(&challenge, &state, "elph");
    let server = start_callback_server(1455, "/auth/callback", Some(&state), "OpenAI authentication completed").await?;

    callbacks.notify(AuthEvent::AuthUrl {
        url: auth_url,
        instructions: Some("A browser window should open. Complete login to finish.".to_string()),
    });

    let callbacks_for_manual = callbacks.clone();
    let state_for_manual = state.clone();
    let callback = tokio::select! {
        result = server.wait_for_code(std::time::Duration::from_secs(600)) => result.map(|r| r.code),
        input = async move {
            callbacks_for_manual
                .prompt(AuthPrompt::ManualCode {
                    message: "Complete login in your browser, or paste the authorization code / redirect URL here:".to_string(),
                    placeholder: Some(REDIRECT_URI.to_string()),
                })
                .await
                .ok()
                .and_then(|input| {
                    let (code, state_parsed) = parse_authorization_input(&input);
                    if let Some(ref s) = state_parsed
                        && s != &state_for_manual {
                            return None;
                        }
                    code
                })
        } => input,
    };

    let code = callback;

    let code = code.ok_or_else(|| anyhow::anyhow!("Missing authorization code"))?;
    exchange_authorization_code(&code, &verifier, REDIRECT_URI).await
}

pub async fn login_openai_codex_device_code(
    callbacks: Arc<dyn AuthLoginCallbacks>,
) -> anyhow::Result<CodexOAuthTokens> {
    let device = start_device_auth().await?;
    callbacks.notify(AuthEvent::DeviceCode {
        user_code: device.user_code.clone(),
        verification_uri: DEVICE_VERIFICATION_URI.to_string(),
        interval_seconds: Some(device.interval_seconds as u32),
        expires_in_seconds: Some(DEVICE_CODE_TIMEOUT_SECONDS as u32),
    });
    let token = poll_device_auth(&device).await?;
    exchange_authorization_code(&token.authorization_code, &token.code_verifier, DEVICE_REDIRECT_URI).await
}

pub async fn refresh_openai_codex_token(refresh_token: &str) -> anyhow::Result<CodexOAuthTokens> {
    let client = reqwest::Client::new();
    let response = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=refresh_token&refresh_token={}&client_id={CLIENT_ID}",
            urlencoding_encode(refresh_token)
        ))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow::anyhow!("OpenAI Codex token refresh failed ({status}): {text}"));
    }
    let json: Value = serde_json::from_str(&text)?;
    tokens_from_json(&json)
}

fn build_authorize_url(challenge: &str, state: &str, originator: &str) -> String {
    format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={CLIENT_ID}&redirect_uri={}&scope={}&code_challenge={challenge}&code_challenge_method=S256&state={state}&id_token_add_organizations=true&codex_cli_simplified_flow=true&originator={originator}",
        urlencoding_encode(REDIRECT_URI),
        urlencoding_encode(SCOPE),
    )
}

fn create_state() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

struct DeviceAuthInfo {
    device_auth_id: String,
    user_code: String,
    interval_seconds: u64,
}

struct DeviceTokenSuccess {
    authorization_code: String,
    code_verifier: String,
}

async fn start_device_auth() -> anyhow::Result<DeviceAuthInfo> {
    let client = reqwest::Client::new();
    let response = client
        .post(DEVICE_USER_CODE_URL)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "client_id": CLIENT_ID }))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if status.as_u16() == 404 {
        return Err(anyhow::anyhow!(
            "OpenAI Codex device code login is not enabled for this server. Use browser login or verify the server URL."
        ));
    }
    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "OpenAI Codex device code request failed ({status}): {text}"
        ));
    }
    let json: Value = serde_json::from_str(&text)?;
    Ok(DeviceAuthInfo {
        device_auth_id: json["device_auth_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid device_auth_id"))?
            .to_string(),
        user_code: json["user_code"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid user_code"))?
            .to_string(),
        interval_seconds: json["interval"].as_u64().unwrap_or(5),
    })
}

async fn poll_device_auth(device: &DeviceAuthInfo) -> anyhow::Result<DeviceTokenSuccess> {
    let device_auth_id = device.device_auth_id.clone();
    let user_code = device.user_code.clone();
    poll_oauth_device_code_flow(DeviceCodePollOptions {
        interval_seconds: Some(device.interval_seconds),
        expires_in_seconds: Some(DEVICE_CODE_TIMEOUT_SECONDS),
        wait_before_first_poll: false,
        poll: Box::new(move || {
            let device_auth_id = device_auth_id.clone();
            let user_code = user_code.clone();
            Box::pin(async move {
                let client = reqwest::Client::new();
                let response = client
                    .post(DEVICE_TOKEN_URL)
                    .header("Content-Type", "application/json")
                    .json(&serde_json::json!({
                        "device_auth_id": device_auth_id,
                        "user_code": user_code,
                    }))
                    .send()
                    .await;

                let response = match response {
                    Ok(r) => r,
                    Err(e) => return DeviceCodePollResult::Failed { message: e.to_string() },
                };

                if response.status().is_success() {
                    let json: Value = match response.json().await {
                        Ok(v) => v,
                        Err(e) => return DeviceCodePollResult::Failed { message: e.to_string() },
                    };
                    let code = json["authorization_code"].as_str();
                    let verifier = json["code_verifier"].as_str();
                    if let (Some(code), Some(verifier)) = (code, verifier) {
                        return DeviceCodePollResult::Complete(DeviceTokenSuccess {
                            authorization_code: code.to_string(),
                            code_verifier: verifier.to_string(),
                        });
                    }
                    return DeviceCodePollResult::Failed {
                        message: format!("Invalid OpenAI Codex device auth token response: {json}"),
                    };
                }

                if response.status().as_u16() == 403 || response.status().as_u16() == 404 {
                    return DeviceCodePollResult::Pending;
                }

                let text = response.text().await.unwrap_or_default();
                let error_code = serde_json::from_str::<Value>(&text).ok().and_then(|j| {
                    j.get("error")
                        .and_then(|e| e.as_str().or_else(|| e.get("code").and_then(|c| c.as_str())))
                        .map(|s| s.to_string())
                });

                match error_code.as_deref() {
                    Some("deviceauth_authorization_pending") => DeviceCodePollResult::Pending,
                    Some("slow_down") => DeviceCodePollResult::SlowDown { interval_seconds: None },
                    _ => DeviceCodePollResult::Failed {
                        message: format!("OpenAI Codex device auth failed: {text}"),
                    },
                }
            }) as Pin<Box<dyn Future<Output = DeviceCodePollResult<DeviceTokenSuccess>> + Send>>
        }),
    })
    .await
}

async fn exchange_authorization_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> anyhow::Result<CodexOAuthTokens> {
    let client = reqwest::Client::new();
    let response = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=authorization_code&client_id={CLIENT_ID}&code={}&code_verifier={}&redirect_uri={}",
            urlencoding_encode(code),
            urlencoding_encode(verifier),
            urlencoding_encode(redirect_uri),
        ))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow::anyhow!("OpenAI Codex token exchange failed ({status}): {text}"));
    }
    let json: Value = serde_json::from_str(&text)?;
    tokens_from_json(&json)
}

fn tokens_from_json(json: &Value) -> anyhow::Result<CodexOAuthTokens> {
    let access = json["access_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing access_token"))?;
    let refresh = json["refresh_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing refresh_token"))?;
    let expires_in = json["expires_in"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("missing expires_in"))?;
    let account_id = get_account_id(access)?;
    Ok(CodexOAuthTokens {
        access: access.to_string(),
        refresh: refresh.to_string(),
        expires: chrono::Utc::now().timestamp_millis() + (expires_in as i64 * 1000),
        account_id,
    })
}

fn get_account_id(access_token: &str) -> anyhow::Result<String> {
    let parts: Vec<&str> = access_token.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow::anyhow!("Failed to extract accountId from token"));
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(parts[1]))?;
    let json: Value = serde_json::from_slice(&payload)?;
    json.pointer(&format!("/{JWT_CLAIM_PATH}/chatgpt_account_id"))
        .or_else(|| json.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("No account ID in token"))
}

fn urlencoding_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

mod hex {
    pub fn encode(bytes: [u8; 16]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
