//! Hugging Face Inference Providers OAuth device-code flow.
//!
//! Ported from osolmaz/pi-huggingface-oauth (`src/oauth.ts`, `src/protocol.ts`,
//! `src/validation.ts`). Device authorization at `https://huggingface.co/oauth/device`,
//! then a token grant at `/oauth/token` with the public `inference-api` scope.
//! The application is public (no client secret); the client ID is configuration,
//! not a secret. Create a client ID at https://huggingface.co/settings/connected-applications.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::auth::OAuthLoader;
use crate::auth::lazy_oauth;
use crate::auth::types::{AuthEvent, AuthLoginCallbacks, OAuthAuth, OAuthCredential};

use super::device_code::{DeviceCodePollOptions, DeviceCodePollResult, poll_oauth_device_code_flow};

const DEVICE_AUTHORIZATION_URL: &str = "https://huggingface.co/oauth/device";
const TOKEN_URL: &str = "https://huggingface.co/oauth/token";
const DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const OAUTH_SCOPE: &str = "inference-api";
const DEFAULT_CLIENT_ID: &str = "548b776f-812d-47a4-b1cb-b55303e0aa51";
const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 5;
const REFRESH_SKEW_MS: i64 = 5 * 60 * 1000;
const OAUTH_FETCH_TIMEOUT_MS: u64 = 15_000;
const REFRESH_RETRY_DELAYS_MS: [u64; 2] = [250, 500];

pub fn huggingface_oauth() -> OAuthAuth {
    lazy_oauth("Hugging Face Inference Providers", huggingface_oauth_loader())
}

pub fn huggingface_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { huggingface_oauth_impl() }))
}

/// Resolve the public OAuth client ID. `ELPH_HUGGINGFACE_OAUTH_CLIENT_ID` and
/// `PI_HUGGINGFACE_OAUTH_CLIENT_ID` (parity with the reference package) override.
pub fn huggingface_client_id() -> String {
    std::env::var("ELPH_HUGGINGFACE_OAUTH_CLIENT_ID")
        .or_else(|_| std::env::var("PI_HUGGINGFACE_OAUTH_CLIENT_ID"))
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

fn huggingface_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "Hugging Face Inference Providers".to_string(),
        login: Arc::new(|callbacks, _identity| {
            Box::pin(async move {
                login_huggingface(&callbacks)
                    .await
                    .map_err(super::map_oauth("Hugging Face login failed"))
            })
        }),
        refresh: Arc::new(|credential| {
            Box::pin(async move {
                refresh_huggingface_token(&credential.refresh)
                    .await
                    .map_err(super::map_oauth("Hugging Face token refresh failed"))
            })
        }),
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

fn credential_expiry(now_ms: i64, expires_in_seconds: u64) -> i64 {
    let lifetime_ms = (expires_in_seconds as i64) * 1000;
    let skew_ms = REFRESH_SKEW_MS.min((lifetime_ms / 10).max(1000));
    now_ms + (lifetime_ms - skew_ms).max(1)
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

#[derive(Debug, Clone)]
struct DeviceAuthResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

async fn post_form(url: &str, fields: Vec<(&str, &str)>) -> anyhow::Result<(bool, serde_json::Value)> {
    let client = reqwest::Client::new();
    let body_str = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish();
    let response = client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body_str)
        .timeout(Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    Ok((status.is_success(), body))
}

async fn request_device_authorization(client_id: &str) -> anyhow::Result<DeviceAuthResponse> {
    let (ok, body) =
        post_form(DEVICE_AUTHORIZATION_URL, vec![("client_id", client_id), ("scope", OAUTH_SCOPE)]).await?;
    if !ok {
        anyhow::bail!("Hugging Face device authorization failed: {}", oauth_error_message(&body));
    }
    Ok(DeviceAuthResponse {
        device_code: required_string(&body, "device_code")?,
        user_code: required_string(&body, "user_code")?,
        verification_uri: body
            .get("verification_uri_complete")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                body.get("verification_uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("https://huggingface.co")
            })
            .to_string(),
        expires_in: body.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(300),
        interval: body
            .get("interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS),
    })
}

pub async fn login_huggingface(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<OAuthCredential> {
    let client_id = huggingface_client_id();
    let device = request_device_authorization(&client_id).await?;

    callbacks.notify(AuthEvent::DeviceCode {
        user_code: device.user_code.clone(),
        verification_uri: device.verification_uri.clone(),
        interval_seconds: Some(device.interval as u32),
        expires_in_seconds: Some(device.expires_in as u32),
    });

    let token = poll_device_token(&client_id, &device).await?;
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis() as i64;
    Ok(oauth_credential(
        token.access,
        token.refresh,
        credential_expiry(now_ms, token.expires_in),
    ))
}

struct TokenGrant {
    access: String,
    refresh: String,
    expires_in: u64,
}

async fn poll_device_token(client_id: &str, device: &DeviceAuthResponse) -> anyhow::Result<TokenGrant> {
    let client_id = client_id.to_string();
    let device_code = device.device_code.clone();

    poll_oauth_device_code_flow(DeviceCodePollOptions {
        interval_seconds: Some(device.interval),
        expires_in_seconds: Some(device.expires_in),
        wait_before_first_poll: true,
        poll: Box::new(move || {
            let client_id = client_id.clone();
            let device_code = device_code.clone();
            Box::pin(async move {
                let fields = vec![
                    ("grant_type", DEVICE_CODE_GRANT_TYPE),
                    ("device_code", &device_code),
                    ("client_id", &client_id),
                ];
                let result = post_form(TOKEN_URL, fields).await;
                let (ok, body) = match result {
                    Ok((ok, body)) => (ok, body),
                    Err(e) => return DeviceCodePollResult::Failed { message: e.to_string() },
                };
                if ok {
                    match parse_token_grant(&body) {
                        Ok(grant) => DeviceCodePollResult::Complete(grant),
                        Err(e) => DeviceCodePollResult::Failed { message: e.to_string() },
                    }
                } else {
                    let error = body.get("error").and_then(|v| v.as_str());
                    match error {
                        Some("authorization_pending") => DeviceCodePollResult::Pending,
                        Some("slow_down") => DeviceCodePollResult::SlowDown {
                            interval_seconds: body.get("interval").and_then(|v| v.as_u64()),
                        },
                        Some("access_denied") => DeviceCodePollResult::Failed {
                            message: "Hugging Face authorization was denied".to_string(),
                        },
                        Some("expired_token") => DeviceCodePollResult::Failed {
                            message: "The Hugging Face device code expired".to_string(),
                        },
                        _ => DeviceCodePollResult::Failed {
                            message: format!("Hugging Face token polling failed: {}", oauth_error_message(&body)),
                        },
                    }
                }
            })
        }),
    })
    .await
}

fn parse_token_grant(body: &serde_json::Value) -> anyhow::Result<TokenGrant> {
    Ok(TokenGrant {
        access: required_string(body, "access_token")?,
        refresh: required_string(body, "refresh_token")?,
        expires_in: body.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(3600),
    })
}

pub async fn refresh_huggingface_token(refresh_token: &str) -> anyhow::Result<OAuthCredential> {
    let client_id = huggingface_client_id();
    let mut attempt = 0;
    loop {
        match refresh_attempt(&client_id, refresh_token).await {
            Ok(grant) => {
                let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis() as i64;
                let refresh = if grant.refresh.is_empty() {
                    refresh_token.to_string()
                } else {
                    grant.refresh
                };
                return Ok(oauth_credential(
                    grant.access,
                    refresh,
                    credential_expiry(now_ms, grant.expires_in),
                ));
            }
            Err(e) if is_retryable(&e) && attempt < REFRESH_RETRY_DELAYS_MS.len() => {
                tokio::time::sleep(Duration::from_millis(REFRESH_RETRY_DELAYS_MS[attempt])).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

fn is_retryable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<RetryableError>().is_some()
}

#[derive(Debug)]
struct RetryableError;

impl std::fmt::Display for RetryableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hugging Face token refresh is temporarily unavailable")
    }
}

impl std::error::Error for RetryableError {}

async fn refresh_attempt(client_id: &str, refresh_token: &str) -> anyhow::Result<TokenGrant> {
    let fields = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];
    let (ok, body) = post_form(TOKEN_URL, fields).await?;
    if !ok {
        let error = body.get("error").and_then(|v| v.as_str()).unwrap_or_default();
        if error == "invalid_grant" {
            anyhow::bail!(
                "The Hugging Face authorization expired or was revoked. Run `/provider connect huggingface` again."
            );
        }
        if error == "temporarily_unavailable" || error.is_empty() {
            return Err(anyhow::anyhow!(RetryableError));
        }
        anyhow::bail!("Hugging Face token refresh failed: {}", oauth_error_message(&body));
    }
    parse_token_grant(&body)
}

fn oauth_error_message(body: &serde_json::Value) -> String {
    let code = body.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
    let description = body.get("error_description").and_then(|v| v.as_str()).unwrap_or("");
    if description.is_empty() {
        code.to_string()
    } else {
        format!("{code}: {description}")
    }
}

fn required_string(json: &serde_json::Value, field: &str) -> anyhow::Result<String> {
    json.get(field)
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("missing {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_defaults_to_public_app() {
        assert_eq!(huggingface_client_id(), DEFAULT_CLIENT_ID);
    }

    #[test]
    fn client_id_reads_override_env() {
        // SAFETY: test-only env mutation; single-threaded unit test.
        unsafe {
            std::env::set_var("ELPH_HUGGINGFACE_OAUTH_CLIENT_ID", "custom-client");
        }
        assert_eq!(huggingface_client_id(), "custom-client");
        unsafe {
            std::env::remove_var("ELPH_HUGGINGFACE_OAUTH_CLIENT_ID");
        }
        assert_eq!(huggingface_client_id(), DEFAULT_CLIENT_ID);
    }

    #[test]
    fn credential_expiry_applies_skew() {
        let now = 1_000_000_000_000i64;
        let expires = credential_expiry(now, 3600);
        assert!(expires > now);
        assert!(expires <= now + 3600 * 1000);
        // 1-year lifetime skew is capped at 5 minutes.
        let year = credential_expiry(now, 366 * 24 * 60 * 60);
        assert!(year >= now + 366 * 24 * 60 * 60 * 1000 - 5 * 60 * 1000);
    }

    #[test]
    fn parse_token_grant_requires_fields() {
        let body = serde_json::json!({ "access_token": "a", "refresh_token": "r", "expires_in": 3600 });
        let grant = parse_token_grant(&body).expect("grant");
        assert_eq!(grant.access, "a");
        assert_eq!(grant.refresh, "r");
        assert_eq!(grant.expires_in, 3600);

        let missing = serde_json::json!({ "access_token": "a" });
        assert!(parse_token_grant(&missing).is_err());
    }
}
