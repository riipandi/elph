use std::sync::Arc;

use thiserror::Error;

use super::types::{
    ApiKeyCredential, AuthContext, AuthModel, AuthResult, BoxFuture, Credential, CredentialStore, OAuthCredential,
    ProviderAuth,
};
use crate::types::ProviderEnv;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelsErrorCode {
    ModelSource,
    ModelValidation,
    Provider,
    Stream,
    Auth,
    OAuth,
}

#[derive(Debug, Error)]
#[error("{code:?}: {message}")]
pub struct ModelsError {
    pub code: ModelsErrorCode,
    pub message: String,
    #[source]
    pub cause: Option<anyhow::Error>,
}

impl ModelsError {
    pub fn new(code: ModelsErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            cause: None,
        }
    }

    pub fn with_cause(code: ModelsErrorCode, message: impl Into<String>, cause: anyhow::Error) -> Self {
        Self {
            code,
            message: message.into(),
            cause: Some(cause),
        }
    }
}

pub struct AuthResolutionOverrides {
    pub api_key: Option<String>,
    pub env: Option<ProviderEnv>,
}

pub async fn resolve_provider_auth(
    provider: &ProviderAuthHolder,
    model: AuthModel,
    credentials: &dyn CredentialStore,
    auth_context: Arc<dyn AuthContext>,
    overrides: Option<AuthResolutionOverrides>,
) -> Result<Option<AuthResult>, ModelsError> {
    let ctx = if let Some(env) = overrides.as_ref().and_then(|o| o.env.clone()) {
        Arc::new(OverlayAuthContext {
            base: auth_context.clone(),
            env,
        }) as Arc<dyn AuthContext>
    } else {
        auth_context
    };

    if let Some(key) = overrides.as_ref().and_then(|o| o.api_key.clone())
        && let Some(api_key) = &provider.auth.api_key
    {
        return resolve_api_key(
            ctx,
            api_key,
            model,
            Some(ApiKeyCredential::new(key)),
            overrides.as_ref().and_then(|o| o.env.clone()),
        )
        .await;
    }

    let stored = credentials.read(&provider.id).await;
    if let Some(stored) = stored {
        return match stored {
            Credential::OAuth(cred) => {
                if let Some(oauth) = &provider.auth.oauth {
                    resolve_stored_oauth(credentials, &provider.id, oauth, cred).await
                } else {
                    Ok(None)
                }
            }
            Credential::ApiKey(cred) => {
                if let Some(api_key) = &provider.auth.api_key {
                    let merged = if let Some(env) = overrides.as_ref().and_then(|o| o.env.clone()) {
                        let mut c = cred.clone();
                        c.env = Some(c.env.unwrap_or_default().into_iter().chain(env).collect());
                        c
                    } else {
                        cred
                    };
                    Ok(resolve_api_key(ctx, api_key, model, Some(merged), None).await?)
                } else {
                    Ok(None)
                }
            }
        };
    }

    if let Some(api_key) = &provider.auth.api_key {
        return resolve_api_key(ctx, api_key, model, None, overrides.and_then(|o| o.env)).await;
    }

    Ok(None)
}

pub struct ProviderAuthHolder {
    pub id: String,
    pub auth: ProviderAuth,
}

struct OverlayAuthContext {
    base: Arc<dyn AuthContext>,
    env: ProviderEnv,
}

impl AuthContext for OverlayAuthContext {
    fn env<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Option<String>> {
        let overlay = self.env.get(name).cloned();
        let base = self.base.clone();
        Box::pin(async move { overlay.or(base.env(name).await) })
    }

    fn file_exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, bool> {
        let base = self.base.clone();
        Box::pin(async move { base.file_exists(path).await })
    }
}

async fn resolve_stored_oauth(
    credentials: &dyn CredentialStore,
    provider_id: &str,
    oauth: &super::types::OAuthAuth,
    mut stored: OAuthCredential,
) -> Result<Option<AuthResult>, ModelsError> {
    if chrono::Utc::now().timestamp_millis() >= stored.expires {
        let oauth = oauth.clone();
        let refresh_result = (oauth.refresh)(stored.clone()).await;
        let refreshed = match refresh_result {
            Ok(next) => next,
            Err(e) => {
                return Err(ModelsError::with_cause(
                    ModelsErrorCode::OAuth,
                    format!("OAuth refresh failed for {provider_id}"),
                    e,
                ));
            }
        };
        let post = credentials
            .modify(
                provider_id,
                Box::new(move |current| {
                    let refreshed = refreshed.clone();
                    Box::pin(async move {
                        let Some(Credential::OAuth(current)) = current else {
                            return None;
                        };
                        if chrono::Utc::now().timestamp_millis() < current.expires {
                            return None;
                        }
                        Some(Credential::OAuth(refreshed))
                    })
                }),
            )
            .await;

        if let Some(Credential::OAuth(cred)) = post {
            stored = cred;
        } else {
            return Ok(None);
        }
    }

    match (oauth.to_auth)(stored).await {
        Ok(auth) => Ok(Some(AuthResult {
            auth,
            env: None,
            source: Some("OAuth".to_string()),
        })),
        Err(e) => Err(ModelsError::with_cause(
            ModelsErrorCode::OAuth,
            format!("OAuth auth derivation failed for {provider_id}"),
            e,
        )),
    }
}

async fn resolve_api_key(
    ctx: Arc<dyn AuthContext>,
    auth: &super::types::ApiKeyAuth,
    model: AuthModel,
    credential: Option<ApiKeyCredential>,
    env_override: Option<ProviderEnv>,
) -> Result<Option<AuthResult>, ModelsError> {
    let input = super::types::AuthResolveInput { model, ctx, credential };
    if let Some(mut result) = (auth.resolve)(input).await {
        if let Some(env) = env_override {
            result.env = Some(result.env.unwrap_or_default().into_iter().chain(env).collect());
        }
        Ok(Some(result))
    } else {
        Ok(None)
    }
}
