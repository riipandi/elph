use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::types::{ApiKeyAuth, ApiKeyCredential, AuthLoginCallbacks, AuthModel, AuthResolveInput, AuthResult};
use super::types::{ModelAuth, OAuthAuth};

pub fn env_api_key_auth(name: impl Into<String>, env_vars: Vec<&'static str>) -> ApiKeyAuth {
    let name = name.into();
    ApiKeyAuth {
        name: name.clone(),
        resolve: Arc::new(move |input: AuthResolveInput| {
            let env_vars = env_vars.clone();
            Box::pin(async move {
                if let Some(key) = input.credential.as_ref().and_then(|c| c.key.clone()) {
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
                // Check the credential's embedded env map (from env-ref entries).
                if let Some(cred) = &input.credential
                    && let Some(ref env) = cred.env
                {
                    for var_name in env.keys() {
                        if let Some(value) = input.ctx.env(var_name).await {
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
                for var in env_vars {
                    if let Some(value) = input.ctx.env(var).await {
                        return Some(AuthResult {
                            auth: ModelAuth {
                                api_key: Some(value),
                                headers: None,
                                base_url: None,
                            },
                            env: None,
                            source: Some(var.to_string()),
                        });
                    }
                }
                None
            })
        }),
        login: Some(Arc::new(move |callbacks: Arc<dyn AuthLoginCallbacks>| {
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
        })),
    }
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
