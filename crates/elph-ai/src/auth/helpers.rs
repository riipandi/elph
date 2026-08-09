use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::types::{ApiKeyAuth, ApiKeyCredential, AuthLoginCallbacks, AuthModel, AuthResolveInput, AuthResult};
use super::types::{ModelAuth, OAuthAuth};

pub fn env_api_key_auth(name: impl Into<String>, env_vars: Vec<&'static str>) -> ApiKeyAuth {
    let owned: Vec<String> = env_vars.into_iter().map(str::to_string).collect();
    flexible_api_key_auth(name, owned)
}

/// GitHub Copilot API-key / env auth: accepts a Copilot session token **or** a GitHub
/// OAuth/PAT and exchanges the latter via `/copilot_internal/v2/token`.
pub fn github_copilot_api_key_auth() -> ApiKeyAuth {
    let env_vars = vec!["COPILOT_GITHUB_TOKEN".to_string(), "GITHUB_TOKEN".to_string()];
    ApiKeyAuth {
        name: "GitHub Copilot token".to_string(),
        resolve: Arc::new(move |input: AuthResolveInput| {
            let env_vars = env_vars.clone();
            Box::pin(async move {
                let mut raw: Option<String> = None;
                let mut source = String::new();
                if let Some(key) = input.credential.as_ref().and_then(|c| c.key.clone())
                    && !key.is_empty()
                {
                    raw = Some(key);
                    source = "stored credential".into();
                }
                if raw.is_none()
                    && let Some(cred) = &input.credential
                    && let Some(ref env) = cred.env
                {
                    for var_name in env.keys() {
                        if let Some(value) = input.ctx.env(var_name).await
                            && !value.is_empty()
                        {
                            raw = Some(value);
                            source = format!("env:{var_name}");
                            break;
                        }
                    }
                }
                if raw.is_none() {
                    for var in &env_vars {
                        if let Some(value) = input.ctx.env(var).await
                            && !value.is_empty()
                        {
                            raw = Some(value);
                            source = var.clone();
                            break;
                        }
                    }
                }
                let Some(token) = raw else {
                    return None;
                };
                match crate::auth::oauth::ensure_copilot_session_token(&token, None).await {
                    Ok(session) => {
                        let base = crate::auth::oauth::get_github_copilot_base_url(Some(&session), None);
                        Some(AuthResult {
                            auth: ModelAuth {
                                api_key: Some(session),
                                headers: None,
                                base_url: Some(base),
                            },
                            env: None,
                            source: Some(source),
                        })
                    }
                    Err(e) => {
                        log::warn!("GitHub Copilot token exchange failed: {e:#}");
                        None
                    }
                }
            })
        }),
        login: None,
    }
}

/// API-key auth that succeeds with an empty key when no env/credential is set.
///
/// Used for local / self-hosted OpenAI-compatible endpoints (Ollama, LM Studio, …)
/// that typically ignore or accept a dummy `Authorization` header.
pub fn optional_env_api_key_auth(name: impl Into<String>, env_vars: Vec<String>) -> ApiKeyAuth {
    flexible_api_key_auth_with_options(name, env_vars, true)
}

/// API-key auth with runtime-owned env var names (for disk-only / custom providers).
///
/// Resolution order: stored credential key → credential env map → process env vars.
pub fn flexible_api_key_auth(name: impl Into<String>, env_vars: Vec<String>) -> ApiKeyAuth {
    flexible_api_key_auth_with_options(name, env_vars, false)
}

fn flexible_api_key_auth_with_options(
    name: impl Into<String>,
    env_vars: Vec<String>,
    allow_missing: bool,
) -> ApiKeyAuth {
    let name = name.into();
    ApiKeyAuth {
        name: name.clone(),
        resolve: Arc::new(move |input: AuthResolveInput| {
            let env_vars = env_vars.clone();
            Box::pin(async move {
                if let Some(key) = input.credential.as_ref().and_then(|c| c.key.clone()) {
                    if !key.is_empty() {
                        return Some(AuthResult {
                            auth: ModelAuth {
                                api_key: Some(key),
                                headers: None,
                                base_url: None,
                            },
                            env: None,
                            source: Some("stored credential".to_string()),
                        });
                    }
                }
                // Check the credential's embedded env map (from env-ref entries).
                if let Some(cred) = &input.credential
                    && let Some(ref env) = cred.env
                {
                    for var_name in env.keys() {
                        if let Some(value) = input.ctx.env(var_name).await
                            && !value.is_empty()
                        {
                            return Some(AuthResult {
                                auth: ModelAuth {
                                    api_key: Some(value),
                                    headers: None,
                                    base_url: None,
                                },
                                env: None,
                                source: Some(format!("env:{var_name}")),
                            });
                        }
                    }
                }
                for var in &env_vars {
                    if let Some(value) = input.ctx.env(var).await
                        && !value.is_empty()
                    {
                        return Some(AuthResult {
                            auth: ModelAuth {
                                api_key: Some(value),
                                headers: None,
                                base_url: None,
                            },
                            env: None,
                            source: Some(var.clone()),
                        });
                    }
                }
                if allow_missing {
                    // Empty bearer — local OpenAI-compatible servers often ignore it.
                    return Some(AuthResult {
                        auth: ModelAuth {
                            api_key: Some(String::new()),
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some("no-auth (optional)".to_string()),
                    });
                }
                None
            })
        }),
        login: if allow_missing {
            None
        } else {
            Some(Arc::new(move |callbacks: Arc<dyn AuthLoginCallbacks>| {
                let name = name.clone();
                Box::pin(async move {
                    let key = callbacks
                        .prompt(super::types::AuthPrompt::Secret {
                            message: format!("Enter {name}"),
                            placeholder: None,
                        })
                        .await?;
                    Ok(ApiKeyCredential::new(key))
                })
            }))
        },
    }
}

/// True when a base URL points at a local/self-hosted endpoint that usually needs no cloud API key.
pub fn is_local_or_loopback_base_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.contains("localhost")
        || lower.contains("127.0.0.1")
        || lower.contains("[::1]")
        || lower.contains("0.0.0.0")
        || lower.starts_with("http://192.168.")
        || lower.starts_with("http://10.")
        || lower.starts_with("http://172.16.")
        || lower.starts_with("http://172.17.")
        || lower.starts_with("http://172.18.")
        || lower.starts_with("http://172.19.")
        || lower.starts_with("http://172.2")
        || lower.starts_with("http://172.3")
}

pub fn lazy_oauth(name: impl Into<String>, load: OAuthLoader) -> OAuthAuth {
    let name = name.into();
    let inner: Arc<tokio::sync::Mutex<Option<Arc<OAuthAuth>>>> = Arc::new(tokio::sync::Mutex::new(None));
    let load_login = load.clone();
    let load_refresh = load.clone();
    let load_to_auth = load;
    let inner_login = inner.clone();
    let inner_refresh = inner.clone();
    let inner_to_auth = inner;

    OAuthAuth {
        name: name.clone(),
        login: Arc::new(move |callbacks| {
            let inner = inner_login.clone();
            let load = load_login.clone();
            Box::pin(async move {
                let auth = loaded(&inner, &load).await;
                (auth.login)(callbacks).await
            })
        }),
        refresh: Arc::new(move |credential| {
            let inner = inner_refresh.clone();
            let load = load_refresh.clone();
            Box::pin(async move {
                let auth = loaded(&inner, &load).await;
                (auth.refresh)(credential).await
            })
        }),
        to_auth: Arc::new(move |credential| {
            let inner = inner_to_auth.clone();
            let load = load_to_auth.clone();
            Box::pin(async move {
                let auth = loaded(&inner, &load).await;
                (auth.to_auth)(credential).await
            })
        }),
    }
}

pub type OAuthLoader = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = OAuthAuth> + Send>> + Send + Sync>;

async fn loaded(slot: &Arc<tokio::sync::Mutex<Option<Arc<OAuthAuth>>>>, load: &OAuthLoader) -> Arc<OAuthAuth> {
    let mut guard = slot.lock().await;
    if guard.is_none() {
        *guard = Some(Arc::new(load().await));
    }
    guard.clone().unwrap()
}

pub fn auth_model_provider(model: &AuthModel) -> &str {
    match model {
        AuthModel::Chat(m) => &m.provider,
        AuthModel::Images(m) => &m.provider,
    }
}
