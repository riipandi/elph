use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::auth::types::{AuthEvent, AuthLoginCallbacks, AuthPrompt, ModelAuth, OAuthAuth, OAuthCredential};
use crate::auth::{OAuthLoader, lazy_oauth};
use crate::models::catalog::GITHUB_COPILOT_MODELS;

use super::device_code::{DeviceCodePollOptions, DeviceCodePollResult, poll_oauth_device_code_flow};

const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const COPILOT_API_VERSION: &str = "2026-06-01";
const COPILOT_HEADERS: &[(&str, &str)] = &[
    ("User-Agent", "GitHubCopilotChat/0.35.0"),
    ("Editor-Version", "vscode/1.107.0"),
    ("Editor-Plugin-Version", "copilot-chat/0.35.0"),
    ("Copilot-Integration-Id", "vscode-chat"),
];

pub fn github_copilot_oauth() -> OAuthAuth {
    lazy_oauth("GitHub Copilot", github_copilot_oauth_loader())
}

pub fn github_copilot_oauth_loader() -> OAuthLoader {
    Arc::new(|| Box::pin(async { github_copilot_oauth_impl() }))
}

fn github_copilot_oauth_impl() -> OAuthAuth {
    OAuthAuth {
        name: "GitHub Copilot".to_string(),
        login: Arc::new(|callbacks: Arc<dyn AuthLoginCallbacks>| {
            Box::pin(async move {
                let creds = login_github_copilot(&callbacks).await?;
                Ok(to_oauth_credential(creds))
            })
        }),
        refresh: Arc::new(|credential| {
            Box::pin(async move {
                let creds =
                    refresh_github_copilot_token(&credential.refresh, credential.enterprise_url.as_deref()).await?;
                Ok(to_oauth_credential(creds))
            })
        }),
        to_auth: Arc::new(|credential| {
            Box::pin(async move {
                let enterprise_domain = copilot_enterprise_domain(&credential);
                Ok(ModelAuth {
                    api_key: Some(credential.access.clone()),
                    headers: None,
                    base_url: Some(get_github_copilot_base_url(
                        Some(&credential.access),
                        enterprise_domain.as_deref(),
                    )),
                })
            })
        }),
    }
}

#[derive(Debug, Clone)]
struct CopilotOAuthTokens {
    access: String,
    refresh: String,
    expires: i64,
    enterprise_url: Option<String>,
    available_model_ids: Vec<String>,
}

fn to_oauth_credential(creds: CopilotOAuthTokens) -> OAuthCredential {
    OAuthCredential {
        kind: "oauth".to_string(),
        access: creds.access,
        refresh: creds.refresh,
        expires: creds.expires,
        account_id: None,
        enterprise_url: creds.enterprise_url,
        available_model_ids: Some(creds.available_model_ids),
    }
}

pub fn normalize_domain(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(url) = url::Url::parse(trimmed) {
        return url.host_str().map(|s| s.to_string());
    }
    if let Ok(url) = url::Url::parse(&format!("https://{trimmed}")) {
        return url.host_str().map(|s| s.to_string());
    }
    None
}

fn copilot_enterprise_domain(credential: &OAuthCredential) -> Option<String> {
    credential.enterprise_url.as_deref().and_then(normalize_domain)
}

pub fn get_github_copilot_base_url(token: Option<&str>, enterprise_domain: Option<&str>) -> String {
    if let Some(token) = token {
        if let Some(url) = base_url_from_token(token) {
            return url;
        }
    }
    if let Some(domain) = enterprise_domain {
        return format!("https://copilot-api.{domain}");
    }
    "https://api.individual.githubcopilot.com".to_string()
}

fn base_url_from_token(token: &str) -> Option<String> {
    let proxy = token.split(';').find_map(|part| part.strip_prefix("proxy-ep="))?;
    let api_host = proxy.strip_prefix("proxy.").unwrap_or(proxy);
    Some(format!("https://api.{api_host}"))
}

struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: Option<u64>,
    expires_in: u64,
}

fn copilot_urls(domain: &str) -> (String, String, String) {
    (
        format!("https://{domain}/login/device/code"),
        format!("https://{domain}/login/oauth/access_token"),
        format!("https://api.{domain}/copilot_internal/v2/token"),
    )
}

pub async fn login_github_copilot(callbacks: &Arc<dyn AuthLoginCallbacks>) -> anyhow::Result<CopilotOAuthTokens> {
    let input = callbacks
        .prompt(AuthPrompt::Text {
            message: "GitHub Enterprise URL/domain (blank for github.com)".to_string(),
            placeholder: Some("company.ghe.com".to_string()),
        })
        .await?;

    let trimmed = input.trim();
    let enterprise_domain = normalize_domain(&input);
    if !trimmed.is_empty() && enterprise_domain.is_none() {
        return Err(anyhow::anyhow!("Invalid GitHub Enterprise URL/domain"));
    }
    let domain = enterprise_domain.as_deref().unwrap_or("github.com");

    let device = start_device_flow(domain).await?;
    callbacks.notify(AuthEvent::DeviceCode {
        user_code: device.user_code.clone(),
        verification_uri: device.verification_uri.clone(),
        interval_seconds: device.interval.map(|i| i as u32),
        expires_in_seconds: Some(device.expires_in as u32),
    });

    let github_access = poll_github_access_token(domain, &device).await?;
    let mut creds = refresh_copilot_access_token(&github_access, enterprise_domain.as_deref()).await?;

    callbacks.notify(AuthEvent::Progress {
        message: "Enabling models...".to_string(),
    });
    enable_all_copilot_models(&creds.access, enterprise_domain.as_deref()).await;
    creds.available_model_ids = fetch_available_model_ids(&creds.access, enterprise_domain.as_deref()).await?;
    Ok(creds)
}

pub async fn refresh_github_copilot_token(
    refresh_token: &str,
    enterprise_domain: Option<&str>,
) -> anyhow::Result<CopilotOAuthTokens> {
    let mut creds = refresh_copilot_access_token(refresh_token, enterprise_domain).await?;
    creds.available_model_ids = fetch_available_model_ids(&creds.access, enterprise_domain).await?;
    Ok(creds)
}

async fn start_device_flow(domain: &str) -> anyhow::Result<DeviceCodeResponse> {
    let (device_code_url, _, _) = copilot_urls(domain);
    let client = reqwest::Client::new();
    let response = client
        .post(&device_code_url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("User-Agent", "GitHubCopilotChat/0.35.0")
        .body(format!("client_id={CLIENT_ID}&scope=read:user"))
        .send()
        .await?;
    let data: serde_json::Value = response.json().await?;
    Ok(DeviceCodeResponse {
        device_code: data["device_code"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid device_code"))?
            .to_string(),
        user_code: data["user_code"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid user_code"))?
            .to_string(),
        verification_uri: data["verification_uri"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid verification_uri"))?
            .to_string(),
        interval: data["interval"].as_u64(),
        expires_in: data["expires_in"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("invalid expires_in"))?,
    })
}

async fn poll_github_access_token(domain: &str, device: &DeviceCodeResponse) -> anyhow::Result<String> {
    let (_, access_token_url, _) = copilot_urls(domain);
    poll_oauth_device_code_flow(DeviceCodePollOptions {
        interval_seconds: device.interval,
        expires_in_seconds: Some(device.expires_in),
        wait_before_first_poll: true,
        poll: Box::new({
            let device_code = device.device_code.clone();
            let access_token_url = access_token_url.clone();
            move || {
                let device_code = device_code.clone();
                let access_token_url = access_token_url.clone();
                Box::pin(async move {
                    let client = reqwest::Client::new();
                    let response = client
                        .post(&access_token_url)
                        .header("Accept", "application/json")
                        .header("Content-Type", "application/x-www-form-urlencoded")
                        .header("User-Agent", "GitHubCopilotChat/0.35.0")
                        .body(format!(
                            "client_id={CLIENT_ID}&device_code={device_code}&grant_type=urn:ietf:params:oauth:grant-type:device_code"
                        ))
                        .send()
                        .await;

                    let response = match response {
                        Ok(r) => r,
                        Err(e) => return DeviceCodePollResult::Failed { message: e.to_string() },
                    };

                    let data: serde_json::Value = match response.json().await {
                        Ok(v) => v,
                        Err(e) => return DeviceCodePollResult::Failed { message: e.to_string() },
                    };

                    if let Some(token) = data["access_token"].as_str() {
                        return DeviceCodePollResult::Complete(token.to_string());
                    }
                    if let Some(error) = data["error"].as_str() {
                        return match error {
                            "authorization_pending" => DeviceCodePollResult::Pending,
                            "slow_down" => DeviceCodePollResult::SlowDown {
                                interval_seconds: data["interval"].as_u64(),
                            },
                            _ => DeviceCodePollResult::Failed {
                                message: format!("Device flow failed: {error}"),
                            },
                        };
                    }
                    DeviceCodePollResult::Failed {
                        message: "Invalid device token response".to_string(),
                    }
                }) as Pin<Box<dyn Future<Output = DeviceCodePollResult<String>> + Send>>
            }
        }),
    })
    .await
}

async fn refresh_copilot_access_token(
    refresh_token: &str,
    enterprise_domain: Option<&str>,
) -> anyhow::Result<CopilotOAuthTokens> {
    let domain = enterprise_domain.unwrap_or("github.com");
    let (_, _, copilot_token_url) = copilot_urls(domain);
    let client = reqwest::Client::new();
    let mut req = client
        .get(&copilot_token_url)
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {refresh_token}"));
    for (k, v) in COPILOT_HEADERS {
        req = req.header(*k, *v);
    }
    let data: serde_json::Value = req.send().await?.json().await?;
    let token = data["token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("invalid copilot token"))?;
    let expires_at = data["expires_at"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("invalid expires_at"))?;
    Ok(CopilotOAuthTokens {
        access: token.to_string(),
        refresh: refresh_token.to_string(),
        expires: expires_at * 1000 - 5 * 60 * 1000,
        enterprise_url: enterprise_domain.map(|s| s.to_string()),
        available_model_ids: vec![],
    })
}

async fn fetch_available_model_ids(token: &str, enterprise_domain: Option<&str>) -> anyhow::Result<Vec<String>> {
    let base_url = get_github_copilot_base_url(Some(token), enterprise_domain);
    let client = reqwest::Client::new();
    let mut req = client
        .get(format!("{base_url}/models"))
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .header("X-GitHub-Api-Version", COPILOT_API_VERSION);
    for (k, v) in COPILOT_HEADERS {
        req = req.header(*k, *v);
    }
    let data: serde_json::Value = req
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .json()
        .await?;
    let ids = data["data"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Invalid Copilot models response"))?
        .iter()
        .filter_map(|item| {
            let id = item["id"].as_str()?;
            let enabled = item["model_picker_enabled"].as_bool().unwrap_or(false);
            let disabled = item.pointer("/policy/state").and_then(|v| v.as_str()) == Some("disabled");
            let no_tools = item
                .pointer("/capabilities/supports/tool_calls")
                .and_then(|v| v.as_bool())
                == Some(false);
            if enabled && !disabled && !no_tools {
                Some(id.to_string())
            } else {
                None
            }
        })
        .collect();
    Ok(ids)
}

async fn enable_all_copilot_models(token: &str, enterprise_domain: Option<&str>) {
    let base_url = get_github_copilot_base_url(Some(token), enterprise_domain);
    let client = reqwest::Client::new();
    for model in GITHUB_COPILOT_MODELS.iter() {
        let mut req = client
            .post(format!("{base_url}/models/{}/policy", model.id))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {token}"))
            .header("openai-intent", "chat-policy")
            .header("x-interaction-type", "chat-policy");
        for (k, v) in COPILOT_HEADERS {
            req = req.header(*k, *v);
        }
        let _ = req.body(r#"{"state":"enabled"}"#).send().await;
    }
}
