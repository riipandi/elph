//! xAI OAuth device-code flow for Grok models.
//!
//! Ported from: https://github.com/earendil-works/pi/blob/main/packages/ai/src/auth/oauth/xai.ts

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::Value;

use crate::auth::OAuthLoader;
use crate::auth::types::{AuthEvent, AuthLoginCallbacks, OAuthAuth, OAuthCredential};

use super::device_code::{DeviceCodePollOptions, DeviceCodePollResult, poll_oauth_device_code_flow};

const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const XAI_DEVICE_CODE_URL: &str = "https://auth.x.ai/oauth2/device/code";
const XAI_TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
const REFRESH_SKEW_MS: u64 = 5 * 60 * 1000; // 5 minutes before expiry
const DEFAULT_TOKEN_LIFETIME_SECONDS: u64 = 3600;

/// xAI OAuth authentication provider.
pub fn xai_oauth() -> OAuthAuth {
    xai_oauth_impl()
}

/// Loader for xAI OAuth provider.
pub fn xai_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { xai_oauth_impl() }))
}

fn xai_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "xAI (Grok/X subscription)".to_string(),
        login: Arc::new(|callbacks: Arc<dyn AuthLoginCallbacks>| {
            Box::pin(async move {
                let creds = login_xai(&callbacks).await?;
                Ok(creds)
            })
        }),
        refresh: Arc::new(|credential| Box::pin(async move { refresh_xai_token(&credential.refresh).await })),
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

/// OAuth credential for xAI.
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

/// Token response from xAI OAuth server.
#[derive(Debug)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

/// Device code response from xAI OAuth server.
#[derive(Debug)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    interval_seconds: Option<u64>,
    expires_in_seconds: u64,
}

/// Parse device code response from JSON.
fn parse_device_code(body: &Value) -> Result<DeviceCodeResponse> {
    let device_code = body
        .get("device_code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing device_code in response"))?
        .to_string();

    let user_code = body
        .get("user_code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing user_code in response"))?
        .to_string();

    let verification_uri = body
        .get("verification_uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing verification_uri in response"))?;

    // Validate verification URI is HTTPS
    let uri = url::Url::parse(verification_uri)?;
    if uri.scheme() != "https" {
        anyhow::bail!("Untrusted verification URI in xAI OAuth response");
    }

    let verification_uri_complete = body
        .get("verification_uri_complete")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            let uri = url::Url::parse(s).expect("Invalid verification_uri_complete");
            if uri.scheme() != "https" {
                None
            } else {
                Some(s.to_string())
            }
        });

    let interval = body.get("interval").and_then(|v| v.as_u64());
    let interval_seconds = interval.filter(|&i| i > 0);

    let expires_in_seconds = body
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing expires_in in response"))?;

    Ok(DeviceCodeResponse {
        device_code,
        user_code,
        verification_uri: verification_uri.to_string(),
        verification_uri_complete,
        interval_seconds,
        expires_in_seconds,
    })
}

/// Parse token response from JSON.
fn parse_token_response(body: &Value, previous_refresh_token: Option<&str>) -> Result<TokenResponse> {
    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing access_token in response"))?
        .to_string();

    // xAI may omit refresh_token on refresh when the token is not rotated
    let refresh_token = if body.get("refresh_token").is_none() && previous_refresh_token.is_some() {
        previous_refresh_token.map(|s| s.to_string())
    } else {
        body.get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
    .ok_or_else(|| anyhow::anyhow!("Missing refresh_token in response"))?;

    let expires_in = body.get("expires_in").and_then(|v| v.as_u64());

    Ok(TokenResponse {
        access_token,
        refresh_token: Some(refresh_token),
        expires_in,
    })
}

/// Make a POST request to xAI OAuth endpoints.
async fn post_form(url: &str, fields: Vec<(&str, &str)>) -> Result<(bool, Value)> {
    use reqwest::Client;

    let client = Client::new();
    let body_str = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish();
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

/// Request device code from xAI OAuth server.
async fn request_device_code() -> Result<DeviceCodeResponse> {
    let product = crate::types::ClientIdentity::default().product;
    let fields = vec![
        ("client_id", XAI_CLIENT_ID),
        ("scope", XAI_SCOPE),
        ("referrer", product.as_str()),
    ];

    let (ok, body) = post_form(XAI_DEVICE_CODE_URL, fields).await?;
    if !ok {
        let error = body.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
        let description = body
            .get("error_description")
            .and_then(|v| v.as_str())
            .map(|s| format!(": {s}"))
            .unwrap_or_default();
        anyhow::bail!("xAI OAuth device authorization failed: {}{}", error, description);
    }

    parse_device_code(&body)
}

/// Poll for tokens using device code.
async fn poll_for_tokens(device: DeviceCodeResponse) -> Result<OAuthCredential> {
    let expires_in_seconds = device.expires_in_seconds;

    let result = poll_oauth_device_code_flow::<OAuthCredential>(DeviceCodePollOptions {
        interval_seconds: device.interval_seconds,
        expires_in_seconds: Some(expires_in_seconds),
        wait_before_first_poll: true,
        poll: Box::new(move || {
            let device_code = device.device_code.clone();
            Box::pin(async move {
                let fields = vec![
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", XAI_CLIENT_ID),
                    ("device_code", &device_code),
                ];

                let poll_result = async {
                    let (ok, body) = post_form(XAI_TOKEN_URL, fields).await?;
                    if ok {
                        let response = parse_token_response(&body, None)?;
                        let expires = response.expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME_SECONDS);
                        // expires is stored as epoch **milliseconds** (same as Pi / other OAuth providers).
                        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;
                        let credential = oauth_credential(
                            response.access_token,
                            response.refresh_token.unwrap_or_default(),
                            now_ms + (expires as i64 * 1000) - REFRESH_SKEW_MS as i64,
                        );
                        Ok::<DeviceCodePollResult<OAuthCredential>, anyhow::Error>(DeviceCodePollResult::Complete(
                            credential,
                        ))
                    } else {
                        let error = body.get("error").and_then(|v| v.as_str());
                        match error {
                            Some("authorization_pending") => {
                                Ok::<DeviceCodePollResult<OAuthCredential>, anyhow::Error>(
                                    DeviceCodePollResult::Pending,
                                )
                            }
                            Some("slow_down") => {
                                let interval = body.get("interval").and_then(|v| v.as_u64());
                                Ok::<DeviceCodePollResult<OAuthCredential>, anyhow::Error>(
                                    DeviceCodePollResult::SlowDown {
                                        interval_seconds: interval,
                                    },
                                )
                            }
                            Some("access_denied") | Some("authorization_denied") => {
                                Ok::<DeviceCodePollResult<OAuthCredential>, anyhow::Error>(
                                    DeviceCodePollResult::Failed {
                                        message: "xAI device authorization was denied".to_string(),
                                    },
                                )
                            }
                            Some("expired_token") => Ok::<DeviceCodePollResult<OAuthCredential>, anyhow::Error>(
                                DeviceCodePollResult::Failed {
                                    message: "xAI device code expired".to_string(),
                                },
                            ),
                            _ => Ok::<DeviceCodePollResult<OAuthCredential>, anyhow::Error>(
                                DeviceCodePollResult::Failed {
                                    message: "xAI OAuth device token polling failed".to_string(),
                                },
                            ),
                        }
                    }
                };

                match poll_result.await {
                    Ok(result) => result,
                    Err(e) => DeviceCodePollResult::Failed { message: e.to_string() },
                }
            })
        }),
    })
    .await?;

    Ok(result)
}

/// Login to xAI using device code flow.
async fn login_xai(callbacks: &Arc<dyn AuthLoginCallbacks>) -> Result<OAuthCredential> {
    let device = request_device_code().await?;

    callbacks.notify(AuthEvent::DeviceCode {
        user_code: device.user_code.clone(),
        verification_uri: device
            .verification_uri_complete
            .clone()
            .unwrap_or(device.verification_uri.clone()),
        interval_seconds: device.interval_seconds.map(|s| s as u32),
        expires_in_seconds: Some(device.expires_in_seconds as u32),
    });

    poll_for_tokens(device).await
}

/// Refresh xAI OAuth token.
async fn refresh_xai_token(refresh_token: &str) -> Result<OAuthCredential> {
    let fields = vec![
        ("grant_type", "refresh_token"),
        ("client_id", XAI_CLIENT_ID),
        ("refresh_token", refresh_token),
    ];

    let (ok, body) = post_form(XAI_TOKEN_URL, fields).await?;
    if !ok {
        let error = body.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
        let description = body
            .get("error_description")
            .and_then(|v| v.as_str())
            .map(|s| format!(": {s}"))
            .unwrap_or_default();
        anyhow::bail!("xAI OAuth token refresh failed: {}{}", error, description);
    }

    let response = parse_token_response(&body, Some(refresh_token))?;
    let expires = response.expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME_SECONDS);
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;

    Ok(oauth_credential(
        response.access_token,
        response.refresh_token.unwrap_or(refresh_token.to_string()),
        now_ms + (expires as i64 * 1000) - REFRESH_SKEW_MS as i64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expires_is_epoch_milliseconds_not_seconds() {
        // A correctly-skewed 1h token must be in the future as ms (≈ 1.7e12 scale).
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
        let expires = now_ms + 3_600_000 - REFRESH_SKEW_MS as i64;
        assert!(expires > 1_000_000_000_000, "expires should be ms epoch, got {expires}");
        assert!(expires > chrono::Utc::now().timestamp_millis());
    }
}
