//! Kilo Gateway OAuth device-code flow.
//!
//! Ported from Kilo-Org/kilo-pi-provider `src/auth.ts` / `src/api.ts`.
//! Device authorization at `{KILO_API_URL}/api/device-auth/codes`, then a
//! long-lived (1 year) token used against the OpenAI-compatible gateway at
//! `{KILO_API_URL}/api/gateway`. Organization accounts are selected after
//! login and billed via the `X-KiloCode-OrganizationId` header.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::OAuthLoader;
use crate::auth::lazy_oauth;
use crate::auth::types::{
    AuthEvent, AuthLoginCallbacks, AuthPrompt, AuthSelectOption, ModelAuth, OAuthAuth, OAuthCredential,
};

use super::device_code::{DeviceCodePollOptions, DeviceCodePollResult, poll_oauth_device_code_flow};

const DEFAULT_KILO_URL: &str = "https://api.kilo.ai";
const KILO_DEVICE_AUTH_PATH: &str = "/api/device-auth/codes";
const KILO_PROFILE_PATH: &str = "/api/profile";
const KILO_GATEWAY_PATH: &str = "/api/gateway";
const KILO_ORG_HEADER: &str = "X-KiloCode-OrganizationId";
const POLL_INTERVAL_SECONDS: u64 = 3;
const TOKEN_EXPIRY_MS: i64 = 365 * 24 * 60 * 60 * 1000; // 1 year
const OAUTH_FETCH_TIMEOUT_MS: u64 = 30_000;

pub fn kilo_oauth() -> OAuthAuth {
    lazy_oauth("Kilo", kilo_oauth_loader())
}

pub fn kilo_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { kilo_oauth_impl() }))
}

/// Root Kilo API URL without trailing slash (`KILO_API_URL` overrides).
pub fn kilo_base_url() -> String {
    std::env::var("KILO_API_URL")
        .ok()
        .map(|raw| raw.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_KILO_URL.to_string())
}

/// OpenAI-compatible gateway base (`{kilo_base_url}/api/gateway`).
pub fn kilo_api_base_url() -> String {
    format!("{}{KILO_GATEWAY_PATH}", kilo_base_url())
}

/// Organization id from `KILO_ORG_ID` / `KILOCODE_ORGANIZATION_ID`.
pub fn kilo_org_id() -> Option<String> {
    std::env::var("KILO_ORG_ID")
        .or_else(|_| std::env::var("KILOCODE_ORGANIZATION_ID"))
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn kilo_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "Kilo".to_string(),
        login: Arc::new(|callbacks, _identity| {
            Box::pin(async move {
                login_kilo(&callbacks)
                    .await
                    .map_err(super::map_oauth("Kilo login failed"))
            })
        }),
        refresh: Arc::new(|credential| {
            Box::pin(async move {
                refresh_kilo_token(&credential)
                    .await
                    .map_err(super::map_oauth("Kilo token refresh failed"))
            })
        }),
        to_auth: Arc::new(|credential| {
            Box::pin(async move {
                let mut headers = HashMap::new();
                if let Some(org_id) = credential.account_id.as_deref() {
                    headers.insert(KILO_ORG_HEADER.to_string(), Some(org_id.to_string()));
                }
                Ok(ModelAuth {
                    api_key: Some(credential.access.clone()),
                    headers: if headers.is_empty() { None } else { Some(headers) },
                    base_url: Some(kilo_api_base_url()),
                })
            })
        }),
    }
}

/// The Kilo device token is long-lived (1 year). Refresh re-uses it until it
/// expires, then requires a fresh login.
pub async fn refresh_kilo_token(credential: &OAuthCredential) -> anyhow::Result<OAuthCredential> {
    if chrono::Utc::now().timestamp_millis() < credential.expires {
        return Ok(credential.clone());
    }
    anyhow::bail!("Kilo token expired. Please run `elph provider connect kilo` to re-authenticate.")
}

pub async fn login_kilo(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<OAuthCredential> {
    let device_auth = initiate_device_auth().await?;
    callbacks.notify(AuthEvent::DeviceCode {
        user_code: device_auth.code.clone(),
        verification_uri: device_auth.verification_url.clone(),
        interval_seconds: Some(POLL_INTERVAL_SECONDS as u32),
        expires_in_seconds: Some(device_auth.expires_in as u32),
    });

    let token = poll_device_auth(&device_auth).await?;
    let organization_id = select_kilo_organization(&token, callbacks).await?;
    Ok(kilo_credential(token, organization_id))
}

fn kilo_credential(token: String, organization_id: Option<String>) -> OAuthCredential {
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis() as i64;
    OAuthCredential {
        kind: "oauth".to_string(),
        access: token.clone(),
        refresh: token,
        expires: now_ms + TOKEN_EXPIRY_MS,
        account_id: organization_id,
        enterprise_url: None,
        available_model_ids: None,
    }
}

#[derive(Debug, Clone)]
struct DeviceAuthResponse {
    code: String,
    verification_url: String,
    expires_in: u64,
}

async fn initiate_device_auth() -> anyhow::Result<DeviceAuthResponse> {
    let client = reqwest::Client::new();
    let url = format!("{}{KILO_DEVICE_AUTH_PATH}", kilo_base_url());
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        anyhow::bail!("Too many pending Kilo authorization requests. Please try again later.");
    }
    let response = response.error_for_status()?;
    let json: serde_json::Value = response.json().await?;
    Ok(DeviceAuthResponse {
        code: required_string(&json, "code")?,
        verification_url: required_string(&json, "verificationUrl")?,
        expires_in: json["expiresIn"].as_u64().unwrap_or(300),
    })
}

async fn poll_device_auth(device_auth: &DeviceAuthResponse) -> anyhow::Result<String> {
    let code = device_auth.code.clone();
    let poll_url = format!("{}{KILO_DEVICE_AUTH_PATH}/{code}", kilo_base_url());

    poll_oauth_device_code_flow(DeviceCodePollOptions {
        interval_seconds: Some(POLL_INTERVAL_SECONDS),
        expires_in_seconds: Some(device_auth.expires_in),
        wait_before_first_poll: true,
        poll: Box::new(move || {
            let poll_url = poll_url.clone();
            Box::pin(async move {
                let client = reqwest::Client::new();
                let response = match client
                    .get(&poll_url)
                    .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
                    .send()
                    .await
                {
                    Ok(response) => response,
                    Err(error) => {
                        return DeviceCodePollResult::Failed {
                            message: format!("Kilo device authorization request failed: {error}"),
                        };
                    }
                };

                let status = response.status();
                if status == reqwest::StatusCode::ACCEPTED {
                    return DeviceCodePollResult::Pending;
                }
                if status == reqwest::StatusCode::FORBIDDEN {
                    return DeviceCodePollResult::Failed {
                        message: "Kilo device authorization was denied".to_string(),
                    };
                }
                if status == reqwest::StatusCode::GONE {
                    return DeviceCodePollResult::Failed {
                        message: "Kilo device authorization code expired".to_string(),
                    };
                }
                if !status.is_success() {
                    return DeviceCodePollResult::Failed {
                        message: format!("Kilo device authorization failed: {status}"),
                    };
                }

                let json: serde_json::Value = match response.json().await {
                    Ok(json) => json,
                    Err(error) => {
                        return DeviceCodePollResult::Failed {
                            message: format!("Kilo device authorization response invalid: {error}"),
                        };
                    }
                };

                let poll_status = json.get("status").and_then(|v| v.as_str()).unwrap_or_default();
                match poll_status {
                    "approved" => match json.get("token").and_then(|v| v.as_str()).filter(|t| !t.is_empty()) {
                        Some(token) => DeviceCodePollResult::Complete(token.to_string()),
                        None => DeviceCodePollResult::Failed {
                            message: "Kilo authorization approved but no token received".to_string(),
                        },
                    },
                    "denied" => DeviceCodePollResult::Failed {
                        message: "Kilo device authorization was denied".to_string(),
                    },
                    "expired" => DeviceCodePollResult::Failed {
                        message: "Kilo device authorization code expired".to_string(),
                    },
                    _ => DeviceCodePollResult::Pending,
                }
            })
        }),
    })
    .await
}

#[derive(Debug, Default)]
struct KiloOrganization {
    id: String,
    name: String,
    role: Option<String>,
}

#[derive(Debug, Default)]
struct KiloProfile {
    organizations: Vec<KiloOrganization>,
}

async fn fetch_kilo_profile(token: &str) -> anyhow::Result<KiloProfile> {
    let client = reqwest::Client::new();
    let url = format!("{}{KILO_PROFILE_PATH}", kilo_base_url());
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_millis(OAUTH_FETCH_TIMEOUT_MS))
        .send()
        .await?
        .error_for_status()?;
    let json: serde_json::Value = response.json().await?;
    let organizations = json
        .get("organizations")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|o| {
                    Some(KiloOrganization {
                        id: o.get("id").and_then(|v| v.as_str())?.to_string(),
                        name: o.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        role: o.get("role").and_then(|v| v.as_str()).map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(KiloProfile { organizations })
}

/// Resolve the organization to bill/filter for. Prefers an env override
/// (`KILO_ORG_ID` / `KILOCODE_ORGANIZATION_ID`), then prompts the user.
async fn select_kilo_organization(
    token: &str,
    callbacks: &Arc<dyn AuthLoginCallbacks>,
) -> anyhow::Result<Option<String>> {
    let env_org = kilo_org_id();
    let profile = match fetch_kilo_profile(token).await {
        Ok(profile) => profile,
        Err(error) => {
            log::warn!("Failed to fetch Kilo profile for organization selection: {error}");
            return Ok(env_org);
        }
    };

    if let Some(env_org) = env_org.as_deref()
        && profile.organizations.iter().any(|org| org.id == env_org)
    {
        return Ok(Some(env_org.to_string()));
    }

    if profile.organizations.is_empty() {
        return Ok(env_org);
    }

    let mut options = vec![AuthSelectOption {
        id: "personal".to_string(),
        label: "Personal Account".to_string(),
        description: None,
    }];
    options.extend(profile.organizations.iter().map(|org| AuthSelectOption {
        id: org.id.clone(),
        label: match &org.role {
            Some(role) => format!("{} ({role})", org.name),
            None => org.name.clone(),
        },
        description: None,
    }));

    let selected = callbacks
        .prompt(AuthPrompt::Select {
            message: "Select Kilo account".to_string(),
            options,
        })
        .await?;

    if selected == "personal" {
        Ok(None)
    } else {
        Ok(Some(selected))
    }
}

fn required_string(json: &serde_json::Value, field: &str) -> anyhow::Result<String> {
    json.get(field)
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("missing {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_defaults_to_api_kilo_ai() {
        let url = kilo_base_url();
        assert_eq!(url, DEFAULT_KILO_URL);
        assert_eq!(kilo_api_base_url(), format!("{DEFAULT_KILO_URL}/api/gateway"));
    }

    #[test]
    fn org_id_reads_kilo_env_vars() {
        // SAFETY: test-only env mutation; single-threaded unit test.
        unsafe {
            std::env::set_var("KILO_ORG_ID", "org_123");
        }
        assert_eq!(kilo_org_id().as_deref(), Some("org_123"));
        unsafe {
            std::env::remove_var("KILO_ORG_ID");
        }
        unsafe {
            std::env::set_var("KILOCODE_ORGANIZATION_ID", "org_456");
        }
        assert_eq!(kilo_org_id().as_deref(), Some("org_456"));
        unsafe {
            std::env::remove_var("KILOCODE_ORGANIZATION_ID");
        }
        assert_eq!(kilo_org_id(), None);
    }

    #[test]
    fn credential_has_one_year_expiry() {
        let cred = kilo_credential("tok".to_string(), Some("org".to_string()));
        assert_eq!(cred.access, "tok");
        assert_eq!(cred.refresh, "tok");
        assert_eq!(cred.account_id.as_deref(), Some("org"));
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis() as i64;
        assert!(cred.expires > now_ms + 360 * 24 * 60 * 60 * 1000);
    }

    #[test]
    fn refresh_reuses_token_until_expiry() {
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis() as i64;
        let cred = OAuthCredential {
            kind: "oauth".to_string(),
            access: "tok".to_string(),
            refresh: "tok".to_string(),
            expires: now_ms + 60_000,
            account_id: None,
            enterprise_url: None,
            available_model_ids: None,
        };
        let rt = tokio::runtime::Runtime::new().expect("rt");
        let refreshed = rt.block_on(refresh_kilo_token(&cred)).expect("refresh");
        assert_eq!(refreshed.access, "tok");

        let expired = OAuthCredential {
            expires: now_ms - 1_000,
            ..cred
        };
        assert!(rt.block_on(refresh_kilo_token(&expired)).is_err());
    }
}
