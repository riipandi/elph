use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::device_code::{DeviceCodePollOptions, DeviceCodePollResult, poll_oauth_device_code_flow};
use crate::auth::types::{AuthLoginCallbacks, ModelAuth, OAuthAuth, OAuthCredential};
use crate::auth::{OAuthLoader, lazy_oauth};

const DEFAULT_HYPER_URL: &str = "https://hyper.charm.land";
const DEFAULT_DEVICE_POLL_INTERVAL_SECONDS: u64 = 5;
const TOKEN_EXPIRY_BUFFER_MS: i64 = 30_000;
const OAUTH_FETCH_TIMEOUT_MS: u64 = 30_000;

pub fn hyper_oauth() -> OAuthAuth {
    lazy_oauth("Charm Hyper", hyper_oauth_loader())
}

pub fn hyper_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { hyper_oauth_impl() }))
}

/// Root Hyper URL without trailing slash (`HYPER_URL` overrides).
pub fn hyper_base_url() -> String {
    std::env::var("HYPER_URL")
        .ok()
        .map(|raw| raw.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_HYPER_URL.to_string())
}

/// OpenAI-compatible API base (`{hyper_base_url}/v1`).
pub fn hyper_api_base_url() -> String {
    format!("{}/v1", hyper_base_url())
}

/// User-Agent sent with Hyper API requests.
pub fn hyper_user_agent() -> String {
    format!("elph-ai/{}", env!("CARGO_PKG_VERSION"))
}

fn hyper_json_headers() -> std::collections::HashMap<String, String> {
    std::collections::HashMap::from([
        ("Content-Type".to_string(), "application/json".to_string()),
        ("User-Agent".to_string(), hyper_user_agent()),
    ])
}

fn hyper_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "Charm Hyper".to_string(),
        login: Arc::new(|callbacks: Arc<dyn AuthLoginCallbacks>| {
            Box::pin(async move {
                let creds = login_hyper(&callbacks).await?;
                Ok(to_oauth_credential(creds))
            })
        }),
        refresh: Arc::new(|credential| {
            Box::pin(async move {
                let creds = refresh_hyper_token(&credential.refresh).await?;
                Ok(to_oauth_credential(creds))
            })
        }),
        to_auth: Arc::new(|credential| {
            Box::pin(async move {
                Ok(ModelAuth {
                    api_key: Some(credential.access.clone()),
                    headers: None,
                    base_url: None,
                })
            })
        }),
    }
}

#[derive(Debug, Clone)]
pub struct HyperOAuthTokens {
    access: String,
    refresh: String,
    expires: i64,
}

fn to_oauth_credential(creds: HyperOAuthTokens) -> OAuthCredential {
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

fn device_name() -> String {
    let host = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("HOST").ok())
        .unwrap_or_default();
    if host.trim().is_empty() {
        "Elph".to_string()
    } else {
        format!("Elph ({host})")
    }
}

pub async fn login_hyper(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<HyperOAuthTokens> {
    let device_auth = initiate_device_auth().await?;
    callbacks.notify(crate::auth::types::AuthEvent::DeviceCode {
        user_code: device_auth.user_code.clone(),
        verification_uri: device_auth.verification_url.clone(),
        interval_seconds: device_auth.interval.map(|value| value as u32),
        expires_in_seconds: Some(device_auth.expires_in as u32),
    });

    let device_token = poll_device_auth(&device_auth).await?;
    let token = exchange_refresh_token(&device_token.refresh_token).await?;
    Ok(token_to_credentials(token, &device_token.refresh_token))
}

pub async fn refresh_hyper_token(refresh_token: &str) -> anyhow::Result<HyperOAuthTokens> {
    let token = exchange_refresh_token(refresh_token).await?;
    Ok(token_to_credentials(token, refresh_token))
}

#[derive(Debug, Clone)]
struct DeviceAuthResponse {
    device_code: String,
    expires_in: u64,
    user_code: String,
    verification_url: String,
    interval: Option<u64>,
}

#[derive(Debug, Clone)]
struct DevicePollSuccess {
    refresh_token: String,
}

async fn initiate_device_auth() -> anyhow::Result<DeviceAuthResponse> {
    let client = reqwest::Client::new();
    let url = format!("{}/device/auth", hyper_base_url());
    let response = client
        .post(url)
        .headers(reqwest_header_map(&hyper_json_headers()))
        .json(&serde_json::json!({ "device_name": device_name() }))
        .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
        .send()
        .await?
        .error_for_status()?;
    let json: serde_json::Value = response.json().await?;
    Ok(DeviceAuthResponse {
        device_code: required_string(&json, "device_code")?,
        expires_in: json["expires_in"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("invalid expires_in"))?,
        user_code: required_string(&json, "user_code")?,
        verification_url: required_string(&json, "verification_url")?,
        interval: json["interval"].as_u64(),
    })
}

async fn poll_device_auth(device_auth: &DeviceAuthResponse) -> anyhow::Result<DevicePollSuccess> {
    let device_code = device_auth.device_code.clone();
    let poll_url = format!("{}/device/auth/{device_code}", hyper_base_url());

    poll_oauth_device_code_flow(DeviceCodePollOptions {
        interval_seconds: device_auth.interval.or(Some(DEFAULT_DEVICE_POLL_INTERVAL_SECONDS)),
        expires_in_seconds: Some(device_auth.expires_in),
        wait_before_first_poll: false,
        poll: Box::new(move || {
            let poll_url = poll_url.clone();
            Box::pin(async move {
                let client = reqwest::Client::new();
                let response = match client
                    .get(&poll_url)
                    .headers(reqwest_header_map(&hyper_json_headers()))
                    .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
                    .send()
                    .await
                {
                    Ok(response) => response,
                    Err(error) => {
                        return DeviceCodePollResult::Failed {
                            message: format!("Hyper device authorization request failed: {error}"),
                        };
                    }
                };

                let status = response.status();
                let json: serde_json::Value = match response.json().await {
                    Ok(json) => json,
                    Err(error) => {
                        return DeviceCodePollResult::Failed {
                            message: format!("Hyper device authorization response invalid: {error}"),
                        };
                    }
                };

                if let Some(refresh_token) = json.get("refresh_token").and_then(|v| v.as_str())
                    && !refresh_token.is_empty()
                {
                    return DeviceCodePollResult::Complete(DevicePollSuccess {
                        refresh_token: refresh_token.to_string(),
                    });
                }

                let error = json.get("error").and_then(|v| v.as_str()).unwrap_or_default();
                match error {
                    "authorization_pending" => DeviceCodePollResult::Pending,
                    "slow_down" => DeviceCodePollResult::SlowDown {
                        interval_seconds: json.get("interval").and_then(|v| v.as_u64()),
                    },
                    "" if status.is_success() => DeviceCodePollResult::Pending,
                    other => DeviceCodePollResult::Failed {
                        message: format!(
                            "Hyper device authorization failed: {}",
                            json.get("error_description").and_then(|v| v.as_str()).unwrap_or(other)
                        ),
                    },
                }
            })
        }),
    })
    .await
}

async fn exchange_refresh_token(refresh_token: &str) -> anyhow::Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let url = format!("{}/token/exchange", hyper_base_url());
    let response = client
        .post(url)
        .headers(reqwest_header_map(&hyper_json_headers()))
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

fn token_to_credentials(token: serde_json::Value, fallback_refresh_token: &str) -> HyperOAuthTokens {
    let access = required_string(&token, "access_token").expect("access_token");
    let refresh = token
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .unwrap_or(fallback_refresh_token)
        .to_string();
    HyperOAuthTokens {
        access,
        refresh,
        expires: token_expires_at_ms(&token),
    }
}

fn token_expires_at_ms(token: &serde_json::Value) -> i64 {
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis() as i64;
    let expires_at = if let Some(expires_in) = token.get("expires_in").and_then(|v| v.as_u64()) {
        now_ms + (expires_in as i64) * 1000
    } else if let Some(expires_at) = token.get("expires_at").and_then(|v| v.as_u64()) {
        (expires_at as i64) * 1000
    } else {
        now_ms + 3_600_000
    };
    let buffer_ms = TOKEN_EXPIRY_BUFFER_MS.min((expires_at - now_ms) / 2);
    expires_at - buffer_ms
}

fn required_string(json: &serde_json::Value, field: &str) -> anyhow::Result<String> {
    json.get(field)
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("missing {field}"))
}

fn reqwest_header_map(headers: &std::collections::HashMap<String, String>) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    for (key, value) in headers {
        if let (Ok(name), Ok(val)) = (
            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
            reqwest::header::HeaderValue::from_str(value),
        ) {
            map.insert(name, val);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_expiry_uses_expires_in() {
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis() as i64;
        let token = serde_json::json!({ "expires_in": 3600 });
        let expires = token_expires_at_ms(&token);
        assert!(expires > now_ms);
        assert!(expires <= now_ms + 3_600_000);
    }
}
