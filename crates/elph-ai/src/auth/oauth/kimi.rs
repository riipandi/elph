//! Kimi Code OAuth device-code flow.
//!
//! Ported from pi-ai `auth/oauth/kimi.ts`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::Value;

use crate::auth::OAuthLoader;
use crate::auth::types::{AuthEvent, AuthLoginCallbacks, OAuthAuth, OAuthCredential};

use super::device_code::{DeviceCodePollOptions, DeviceCodePollResult, poll_oauth_device_code_flow};

const KIMI_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const KIMI_DEFAULT_OAUTH_HOST: &str = "https://auth.kimi.com";
const KIMI_DEVICE_CODE_PATH: &str = "/api/oauth/device_authorization";
const KIMI_TOKEN_PATH: &str = "/api/oauth/token";
const REFRESH_SKEW_MS: i64 = 5 * 60 * 1000;
const DEFAULT_TOKEN_LIFETIME_SECONDS: u64 = 3600;

pub fn kimi_oauth() -> OAuthAuth {
    kimi_oauth_impl()
}

pub fn kimi_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { kimi_oauth_impl() }))
}

fn kimi_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "Kimi Code Subscription".to_string(),
        login: Arc::new(|callbacks: Arc<dyn AuthLoginCallbacks>| {
            Box::pin(async move {
                let creds = login_kimi(&callbacks).await?;
                Ok(creds)
            })
        }),
        refresh: Arc::new(|credential| Box::pin(async move { refresh_kimi_token(&credential.refresh).await })),
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

fn get_oauth_host() -> String {
    std::env::var("KIMI_CODE_OAUTH_HOST")
        .or_else(|_| std::env::var("KIMI_OAUTH_HOST"))
        .unwrap_or_else(|_| KIMI_DEFAULT_OAUTH_HOST.to_string())
        .trim_end_matches('/')
        .to_string()
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

async fn post_form(url: &str, fields: Vec<(&str, &str)>) -> Result<(bool, Value)> {
    let client = reqwest::Client::new();
    let body_str = serde_urlencoded::to_string(fields)?;
    let response = client
        .post(url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body_str)
        .send()
        .await?;
    let status = response.status();
    let body: Value = response.json().await?;
    Ok((status.is_success(), body))
}

async fn login_kimi(callbacks: &Arc<dyn AuthLoginCallbacks>) -> Result<OAuthCredential> {
    let oauth_host = get_oauth_host();
    let device_code_url = format!("{oauth_host}{KIMI_DEVICE_CODE_PATH}");
    let token_url = format!("{oauth_host}{KIMI_TOKEN_PATH}");

    let fields = vec![("client_id", KIMI_CLIENT_ID)];

    let (ok, body) = post_form(&device_code_url, fields).await?;
    if !ok {
        let error = body.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
        let description = body.get("error_description").and_then(|v| v.as_str()).unwrap_or("");
        anyhow::bail!("Kimi OAuth device authorization failed: {error}: {description}");
    }

    let device_code = body
        .get("device_code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing device_code"))?
        .to_string();
    let user_code = body
        .get("user_code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing user_code"))?
        .to_string();
    let verification_uri = body
        .get("verification_uri")
        .and_then(|v| v.as_str())
        .unwrap_or("https://kimi.moonshot.cn/oauth/device")
        .to_string();
    let verification_uri_complete = body
        .get("verification_uri_complete")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let interval_seconds = body.get("interval").and_then(|v| v.as_u64());
    let expires_in_seconds = body.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(300);

    callbacks.notify(AuthEvent::DeviceCode {
        user_code: user_code.clone(),
        verification_uri: verification_uri_complete.clone().unwrap_or(verification_uri.clone()),
        interval_seconds: interval_seconds.map(|s| s as u32),
        expires_in_seconds: Some(expires_in_seconds as u32),
    });

    poll_oauth_device_code_flow::<OAuthCredential>(DeviceCodePollOptions {
        interval_seconds,
        expires_in_seconds: Some(expires_in_seconds),
        wait_before_first_poll: true,
        poll: Box::new(move || {
            let device_code = device_code.clone();
            let token_url = token_url.clone();
            Box::pin(async move {
                let fields = vec![
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", KIMI_CLIENT_ID),
                    ("device_code", &device_code),
                ];

                let result = post_form(&token_url, fields).await;
                let (ok, body) = match result {
                    Ok((ok, body)) => (ok, body),
                    Err(e) => return DeviceCodePollResult::Failed { message: e.to_string() },
                };
                if ok {
                    let access_token = match body.get("access_token").and_then(|v| v.as_str()) {
                        Some(t) => t.to_string(),
                        None => {
                            return DeviceCodePollResult::Failed {
                                message: "Missing access_token".to_string(),
                            };
                        }
                    };
                    let refresh_token = body
                        .get("refresh_token")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let expires_in = body
                        .get("expires_in")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(DEFAULT_TOKEN_LIFETIME_SECONDS);
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let credential = oauth_credential(
                        access_token,
                        refresh_token,
                        now * 1000 + (expires_in as i64 * 1000 - REFRESH_SKEW_MS),
                    );
                    DeviceCodePollResult::Complete(credential)
                } else {
                    let error = body.get("error").and_then(|v| v.as_str());
                    match error {
                        Some("authorization_pending") => DeviceCodePollResult::Pending,
                        Some("slow_down") => {
                            let interval = body.get("interval").and_then(|v| v.as_u64());
                            DeviceCodePollResult::SlowDown {
                                interval_seconds: interval,
                            }
                        }
                        Some("access_denied") | Some("authorization_denied") => DeviceCodePollResult::Failed {
                            message: "Kimi device authorization was denied".to_string(),
                        },
                        Some("expired_token") => DeviceCodePollResult::Failed {
                            message: "Kimi device code expired".to_string(),
                        },
                        _ => DeviceCodePollResult::Failed {
                            message: "Kimi OAuth polling failed".to_string(),
                        },
                    }
                }
            })
        }),
    })
    .await
}

async fn refresh_kimi_token(refresh_token: &str) -> Result<OAuthCredential> {
    let oauth_host = get_oauth_host();
    let token_url = format!("{oauth_host}{KIMI_TOKEN_PATH}");
    let fields = vec![
        ("grant_type", "refresh_token"),
        ("client_id", KIMI_CLIENT_ID),
        ("refresh_token", refresh_token),
    ];

    let (ok, body) = post_form(&token_url, fields).await?;
    if !ok {
        let error = body.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
        anyhow::bail!("Kimi OAuth token refresh failed: {error}");
    }

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing access_token"))?
        .to_string();
    let new_refresh = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or(refresh_token)
        .to_string();
    let expires_in = body
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TOKEN_LIFETIME_SECONDS);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    Ok(oauth_credential(
        access_token,
        new_refresh,
        now * 1000 + (expires_in as i64 * 1000 - REFRESH_SKEW_MS),
    ))
}
