//! Cline / ClinePass OAuth (WorkOS device-code flow).
//!
//! Ported from maxpaulus43/pi-cline `index.ts` / `cline-account.ts`.
//! Device authorization at `api.workos.com/user_management/authorize/device`,
//! token exchange at `/user_management/authenticate`, then the WorkOS tokens
//! are registered with Cline at `/api/v1/auth/register` which issues the
//! Cline-scoped access/refresh pair. `Authorization: Bearer workos:…` calls the
//! OpenAI-compatible gateway at `https://api.cline.bot/api/v1`.
//!
//! The `X-CLIENT-TYPE: pi` header and User-Agent mirror the reference client;
//! the Cline backend validates them.

use std::collections::HashMap;
use std::sync::Arc;

use crate::auth::OAuthLoader;
use crate::auth::lazy_oauth;
use crate::auth::types::{AuthEvent, AuthPrompt};
use crate::auth::types::{AuthLoginCallbacks, AuthSelectOption, ModelAuth, OAuthAuth, OAuthCredential};

use super::device_code::{DeviceCodePollOptions, DeviceCodePollResult, poll_oauth_device_code_flow};

const CLINE_API_BASE_URL: &str = "https://api.cline.bot";
const WORKOS_API_BASE_URL: &str = "https://api.workos.com";
const WORKOS_CLIENT_ID: &str = "client_01K3A541FN8TA3EPPHTD2325AR";
const WORKOS_DEVICE_AUTH_PATH: &str = "/user_management/authorize/device";
const WORKOS_TOKEN_PATH: &str = "/user_management/authenticate";
const DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const REGISTER_PATH: &str = "/api/v1/auth/register";
const REFRESH_PATH: &str = "/api/v1/auth/refresh";
const ME_PATH: &str = "/api/v1/users/me";
const ACTIVE_ACCOUNT_PATH: &str = "/api/v1/users/active-account";
const WORKOS_TOKEN_PREFIX: &str = "workos:";
const CLIENT_TYPE_HEADER: &str = "X-CLIENT-TYPE";
const USER_AGENT: &str = "elph-cline-oauth-extension";
const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 5;
/// Refresh this long before Cline reports the token dead.
const REFRESH_BUFFER_MS: i64 = 5 * 60 * 1000;
const OAUTH_FETCH_TIMEOUT_MS: u64 = 15_000;

/// OAuth for the `cline` provider — prompts personal vs organization after login.
pub fn cline_oauth() -> OAuthAuth {
    lazy_oauth("Cline", cline_oauth_loader(false))
}

/// OAuth for the `cline-pass` provider — always activates the personal account
/// (ClinePass billing is personal-account based).
pub fn cline_pass_oauth() -> OAuthAuth {
    lazy_oauth("ClinePass", cline_oauth_loader(true))
}

pub fn cline_oauth_loader(force_personal_account: bool) -> OAuthLoader {
    Arc::new(move || {
        let force_personal_account = force_personal_account;
        Box::pin(async move { cline_oauth_impl(force_personal_account) })
    })
}

fn client_headers() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::ACCEPT, "application/json".parse().expect("static"));
    headers.insert(
        reqwest::header::HeaderName::from_static("x-client-type"),
        "pi".parse().expect("static"),
    );
    headers.insert(reqwest::header::USER_AGENT, USER_AGENT.parse().expect("static"));
    headers
}

fn cline_url(path: &str) -> String {
    format!("{CLINE_API_BASE_URL}{path}")
}

/// Bearer token sent to Cline APIs: the access token prefixed with `workos:`
/// unless it already carries the prefix.
pub fn cline_api_key(access: &str) -> String {
    if access.to_ascii_lowercase().starts_with(WORKOS_TOKEN_PREFIX) {
        access.to_string()
    } else {
        format!("{WORKOS_TOKEN_PREFIX}{access}")
    }
}

fn cline_oauth_impl(force_personal_account: bool) -> OAuthAuth {
    OAuthAuth {
        name: if force_personal_account {
            "ClinePass".to_string()
        } else {
            "Cline".to_string()
        },
        login: Arc::new(move |callbacks, _identity| {
            Box::pin(async move {
                login_cline_impl(&callbacks, force_personal_account)
                    .await
                    .map_err(super::map_oauth("Cline login failed"))
            })
        }),
        refresh: Arc::new(|credential| {
            Box::pin(async move {
                refresh_cline_token(&credential)
                    .await
                    .map_err(super::map_oauth("Cline token refresh failed"))
            })
        }),
        to_auth: Arc::new(|credential| {
            Box::pin(async move {
                let mut headers = HashMap::new();
                headers.insert(CLIENT_TYPE_HEADER.to_string(), Some("pi".to_string()));
                Ok(ModelAuth {
                    api_key: Some(cline_api_key(&credential.access)),
                    headers: Some(headers),
                    // Models carry `https://api.cline.bot/api/v1`; nothing to override.
                    base_url: None,
                })
            })
        }),
    }
}

pub async fn login_cline(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<OAuthCredential> {
    login_cline_impl(callbacks, false).await
}

pub async fn login_cline_pass(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<OAuthCredential> {
    login_cline_impl(callbacks, true).await
}

async fn login_cline_impl(
    callbacks: &Arc<dyn AuthLoginCallbacks>,
    force_personal_account: bool,
) -> anyhow::Result<OAuthCredential> {
    let device = start_device_authorization().await?;

    callbacks.notify(AuthEvent::DeviceCode {
        user_code: device.user_code.clone(),
        verification_uri: device
            .verification_uri_complete
            .clone()
            .unwrap_or_else(|| device.verification_uri.clone()),
        interval_seconds: Some(device.interval_seconds as u32),
        expires_in_seconds: Some(device.expires_in_seconds as u32),
    });

    let workos_tokens = poll_device_authorization(&device).await?;
    let mut credential =
        register_workos_tokens(&workos_tokens.access_token, &workos_tokens.refresh_token, None).await?;

    let account_id = select_active_account(&credential, callbacks, force_personal_account).await?;
    credential.account_id = account_id;
    Ok(credential)
}

#[derive(Debug, Clone)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in_seconds: u64,
    interval_seconds: u64,
}

async fn post_form(url: &str, fields: Vec<(&str, &str)>) -> anyhow::Result<(bool, serde_json::Value)> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish();
    let response = reqwest::Client::new()
        .post(url)
        .headers(client_headers())
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    Ok((status.is_success(), body))
}

async fn start_device_authorization() -> anyhow::Result<DeviceAuthorization> {
    let (ok, body) = post_form(
        &format!("{WORKOS_API_BASE_URL}{WORKOS_DEVICE_AUTH_PATH}"),
        vec![("client_id", WORKOS_CLIENT_ID)],
    )
    .await?;
    if !ok {
        anyhow::bail!("Cline device authorization failed: {}", oauth_error_message(&body));
    }
    Ok(DeviceAuthorization {
        device_code: required_string(&body, "device_code")?,
        user_code: required_string(&body, "user_code")?,
        verification_uri: required_string(&body, "verification_uri")?,
        verification_uri_complete: optional_string(&body, "verification_uri_complete"),
        expires_in_seconds: body.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(300),
        interval_seconds: body
            .get("interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS),
    })
}

#[derive(Debug, Clone)]
struct WorkOsTokens {
    access_token: String,
    refresh_token: String,
}

async fn poll_device_authorization(device: &DeviceAuthorization) -> anyhow::Result<WorkOsTokens> {
    let device_code = device.device_code.clone();

    poll_oauth_device_code_flow(DeviceCodePollOptions {
        interval_seconds: Some(device.interval_seconds.max(1)),
        expires_in_seconds: Some(device.expires_in_seconds),
        wait_before_first_poll: true,
        poll: Box::new(move || {
            let device_code = device_code.clone();
            Box::pin(async move {
                let result = post_form(
                    &format!("{WORKOS_API_BASE_URL}{WORKOS_TOKEN_PATH}"),
                    vec![
                        ("grant_type", DEVICE_CODE_GRANT_TYPE),
                        ("device_code", &device_code),
                        ("client_id", WORKOS_CLIENT_ID),
                    ],
                )
                .await;
                match result {
                    Err(error) => DeviceCodePollResult::Failed {
                        message: format!("Cline device authorization request failed: {error}"),
                    },
                    Ok((true, body)) => {
                        let access = body.get("access_token").and_then(|v| v.as_str()).unwrap_or_default();
                        let refresh = body.get("refresh_token").and_then(|v| v.as_str()).unwrap_or_default();
                        if access.is_empty() || refresh.is_empty() {
                            DeviceCodePollResult::Failed {
                                message: "Cline authorization approved but no token received".to_string(),
                            }
                        } else {
                            DeviceCodePollResult::Complete(WorkOsTokens {
                                access_token: access.to_string(),
                                refresh_token: refresh.to_string(),
                            })
                        }
                    }
                    Ok((false, body)) => match body.get("error").and_then(|v| v.as_str()) {
                        Some("authorization_pending") => DeviceCodePollResult::Pending,
                        Some("slow_down") => DeviceCodePollResult::SlowDown { interval_seconds: None },
                        Some("access_denied") => DeviceCodePollResult::Failed {
                            message: "Cline authorization was denied".to_string(),
                        },
                        Some("expired_token") => DeviceCodePollResult::Failed {
                            message: "The Cline device code expired".to_string(),
                        },
                        _ => DeviceCodePollResult::Failed {
                            message: format!("Cline device authorization failed: {}", oauth_error_message(&body)),
                        },
                    },
                }
            })
        }),
    })
    .await
}

async fn post_json(url: &str, payload: serde_json::Value) -> anyhow::Result<(bool, serde_json::Value)> {
    let response = reqwest::Client::new()
        .post(url)
        .headers(client_headers())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&payload)
        .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    Ok((status.is_success(), body))
}

/// Exchange WorkOS tokens for the Cline-scoped credential
/// (`POST /api/v1/auth/register`). Keeps `fallback_refresh` when the response
/// omits a refresh token, mirroring the reference client.
async fn register_workos_tokens(
    access_token: &str,
    refresh_token: &str,
    fallback_refresh: Option<&str>,
) -> anyhow::Result<OAuthCredential> {
    let (ok, body) = post_json(
        &cline_url(REGISTER_PATH),
        serde_json::json!({ "accessToken": access_token, "refreshToken": refresh_token }),
    )
    .await?;
    if !ok {
        anyhow::bail!("Cline token registration failed: {}", oauth_error_message(&body));
    }
    cline_credential(&body, fallback_refresh)
}

pub async fn refresh_cline_token(credential: &OAuthCredential) -> anyhow::Result<OAuthCredential> {
    let (ok, body) = post_json(
        &cline_url(REFRESH_PATH),
        serde_json::json!({ "refreshToken": credential.refresh, "grantType": "refresh_token" }),
    )
    .await?;
    if !ok {
        anyhow::bail!("Cline token refresh failed: {}", oauth_error_message(&body));
    }
    cline_credential(&body, Some(&credential.refresh))
}

/// Parse a Cline auth envelope: `{success, data: {accessToken, refreshToken?, expiresAt}}`.
fn cline_credential(body: &serde_json::Value, fallback_refresh: Option<&str>) -> anyhow::Result<OAuthCredential> {
    let success = body.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    let data = body.get("data");
    let access = data.and_then(|d| d.get("accessToken")).and_then(|v| v.as_str());
    let expires_at = data.and_then(|d| d.get("expiresAt")).and_then(|v| v.as_str());
    if !success || access.is_none() || expires_at.is_none() {
        anyhow::bail!("Invalid token response from Cline");
    }
    let refresh = data
        .and_then(|d| d.get("refreshToken"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| fallback_refresh.map(str::to_string))
        .ok_or_else(|| anyhow::anyhow!("Token response did not include a refresh token"))?;
    let expires_at = expires_at.expect("checked above");
    let expires_ms = chrono::DateTime::parse_from_rfc3339(expires_at)
        .map_err(|error| anyhow::anyhow!("Invalid token expiration from Cline: {error}"))?
        .timestamp_millis();
    Ok(OAuthCredential {
        kind: "oauth".to_string(),
        access: access.expect("checked above").to_string(),
        refresh,
        expires: expires_ms - REFRESH_BUFFER_MS,
        account_id: None,
        enterprise_url: None,
        available_model_ids: None,
    })
}

#[derive(Debug, Default)]
struct ClineAccount {
    organizations: Vec<ClineOrganization>,
}

#[derive(Debug)]
struct ClineOrganization {
    id: String,
    name: String,
    roles: Vec<String>,
    active: bool,
}

async fn fetch_cline_me(api_key: &str) -> anyhow::Result<ClineAccount> {
    let response = reqwest::Client::new()
        .get(cline_url(ME_PATH))
        .headers(client_headers())
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"))
        .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
        .send()
        .await?
        .error_for_status()?;
    let json: serde_json::Value = response.json().await?;
    let organizations = json
        .pointer("/data/organizations")
        .or_else(|| json.get("organizations"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|o| {
                    Some(ClineOrganization {
                        id: o.get("organizationId").and_then(|v| v.as_str())?.trim().to_string(),
                        name: o
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                        roles: o
                            .get("roles")
                            .and_then(|v| v.as_array())
                            .map(|roles| {
                                roles
                                    .iter()
                                    .filter_map(|r| r.as_str().map(str::to_string))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default(),
                        active: o.get("active").and_then(|v| v.as_bool()).unwrap_or(false),
                    })
                })
                .filter(|org| !org.id.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Ok(ClineAccount { organizations })
}

/// Activate personal vs an organization account server-side.
async fn switch_cline_organization(api_key: &str, organization_id: Option<&str>) -> anyhow::Result<()> {
    let (ok, body) = post_json_with_auth(
        &cline_url(ACTIVE_ACCOUNT_PATH),
        serde_json::json!({ "organizationId": organization_id }),
        api_key,
    )
    .await?;
    if !ok {
        anyhow::bail!("Cline organization switch failed: {}", oauth_error_message(&body));
    }
    Ok(())
}

async fn post_json_with_auth(
    url: &str,
    payload: serde_json::Value,
    api_key: &str,
) -> anyhow::Result<(bool, serde_json::Value)> {
    let response = reqwest::Client::new()
        .put(url)
        .headers(client_headers())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"))
        .json(&payload)
        .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    Ok((status.is_success(), body))
}

/// Post-login account activation. `force_personal_account` (ClinePass) switches
/// straight to the personal account; otherwise the user picks when orgs exist.
async fn select_active_account(
    credential: &OAuthCredential,
    callbacks: &Arc<dyn AuthLoginCallbacks>,
    force_personal_account: bool,
) -> anyhow::Result<Option<String>> {
    let api_key = cline_api_key(&credential.access);
    if force_personal_account {
        switch_cline_organization(&api_key, None).await?;
        callbacks.notify(AuthEvent::Progress {
            message: "Cline active account: Personal account".to_string(),
        });
        return Ok(None);
    }

    let account = match fetch_cline_me(&api_key).await {
        Ok(account) => account,
        Err(error) => {
            log::warn!("Failed to fetch Cline account for organization selection: {error}");
            return Ok(None);
        }
    };
    if account.organizations.is_empty() {
        return Ok(None);
    }

    let mut options = vec![AuthSelectOption {
        id: "personal".to_string(),
        label: format!(
            "{} Personal Account",
            if account.organizations.iter().any(|org| org.active) {
                " "
            } else {
                "✓"
            }
        ),
        description: None,
    }];
    options.extend(account.organizations.iter().map(|org| AuthSelectOption {
        id: org.id.clone(),
        label: format!(
            "{} {}{}",
            if org.active { "✓" } else { " " },
            org.name,
            match org.roles.as_slice() {
                [] => String::new(),
                roles => format!(" ({})", roles.join(", ")),
            }
        ),
        description: None,
    }));

    let selected = callbacks
        .prompt(AuthPrompt::Select {
            message: "Choose your active Cline account".to_string(),
            options,
        })
        .await?;

    if selected == "personal" {
        switch_cline_organization(&api_key, None).await?;
        callbacks.notify(AuthEvent::Progress {
            message: "Cline active account: Personal account".to_string(),
        });
        return Ok(None);
    }

    let org = account.organizations.iter().find(|org| org.id == selected);
    switch_cline_organization(&api_key, Some(selected.as_str())).await?;
    let name = org.map(|o| o.name.clone()).unwrap_or_else(|| selected.clone());
    callbacks.notify(AuthEvent::Progress {
        message: format!("Cline active account: {name}"),
    });
    Ok(Some(selected))
}

fn oauth_error_message(body: &serde_json::Value) -> String {
    for field in ["error_description", "message", "error"] {
        if let Some(value) = body.get(field).and_then(|v| v.as_str()).filter(|v| !v.is_empty()) {
            return value.to_string();
        }
    }
    "unknown error".to_string()
}

fn required_string(json: &serde_json::Value, field: &str) -> anyhow::Result<String> {
    json.get(field)
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("missing {field}"))
}

fn optional_string(json: &serde_json::Value, field: &str) -> Option<String> {
    json.get(field)
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_adds_workos_prefix_once() {
        assert_eq!(cline_api_key("abc"), "workos:abc");
        assert_eq!(cline_api_key("workos:abc"), "workos:abc");
        assert_eq!(cline_api_key("WORKOS:abc"), "WORKOS:abc");
    }

    #[test]
    fn credential_parses_register_envelope() {
        let body = serde_json::json!({
            "success": true,
            "data": {
                "accessToken": "at",
                "refreshToken": "rt",
                "expiresAt": "2030-01-01T00:00:00Z"
            }
        });
        let cred = cline_credential(&body, None).expect("credential");
        assert_eq!(cred.access, "at");
        assert_eq!(cred.refresh, "rt");
        let expected = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
            .expect("ts")
            .timestamp_millis()
            - REFRESH_BUFFER_MS;
        assert_eq!(cred.expires, expected);
    }

    #[test]
    fn credential_falls_back_to_previous_refresh_token() {
        let body = serde_json::json!({
            "success": true,
            "data": { "accessToken": "at", "expiresAt": "2030-01-01T00:00:00Z" }
        });
        let cred = cline_credential(&body, Some("old-rt")).expect("credential");
        assert_eq!(cred.refresh, "old-rt");
        assert!(cline_credential(&serde_json::json!({ "success": false }), None).is_err());
    }
}
