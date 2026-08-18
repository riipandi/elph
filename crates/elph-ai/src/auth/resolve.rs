use std::fmt;
use std::sync::Arc;

use super::types::{ApiKeyCredential, AuthContext, AuthModel, AuthResult, BoxFuture, Credential, CredentialStore};
use super::types::{OAuthCredential, ProviderAuth};
use crate::types::ProviderEnv;

/// Class of a [`ModelsError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelsErrorCode {
    /// Dynamic model list / catalog refresh failed.
    ModelSource,
    /// A catalog or model record failed validation.
    ModelValidation,
    /// Unknown provider or missing API adapter.
    Provider,
    /// Reserved for stream setup (generation errors stay in-band).
    Stream,
    /// API key / credential store resolution failed.
    Auth,
    /// OAuth login, refresh, or token derivation failed.
    OAuth,
}

/// Out-of-band error: catalog, auth, OAuth login/refresh, or provider lookup.
///
/// Chat/image **generation** failures stay in-band
/// (`AssistantMessageEvent::Error` / `StopReason::Error` / `Aborted`).
pub struct ModelsError {
    pub code: ModelsErrorCode,
    pub message: String,
    pub source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl fmt::Debug for ModelsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModelsError")
            .field("code", &self.code)
            .field("message", &self.message)
            .field("source", &self.source.as_ref().map(ToString::to_string))
            .finish()
    }
}

impl fmt::Display for ModelsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)?;
        if let Some(ref source) = self.source {
            write!(f, " — {source}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ModelsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|e| e as _)
    }
}

impl ModelsError {
    pub fn new(code: ModelsErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        code: ModelsErrorCode,
        message: impl Into<String>,
        source: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            source: Some(source.into()),
        }
    }

    /// OAuth login, refresh, or token derivation failure.
    pub fn oauth(message: impl Into<String>) -> Self {
        Self::new(ModelsErrorCode::OAuth, message)
    }

    /// Wrap an underlying OAuth error.
    pub fn oauth_source(
        message: impl Into<String>,
        source: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    ) -> Self {
        Self::with_source(ModelsErrorCode::OAuth, message, source)
    }
}

pub struct AuthResolutionOverrides {
    pub api_key: Option<String>,
    pub env: Option<ProviderEnv>,
}

#[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.ai.auth"))]
pub async fn resolve_provider_auth(
    provider: &ProviderAuthHolder,
    model: AuthModel,
    credentials: &dyn CredentialStore,
    auth_context: Arc<dyn AuthContext>,
    overrides: Option<AuthResolutionOverrides>,
) -> Result<Option<AuthResult>, ModelsError> {
    crate::trace::add_property("provider.id", provider.id.clone());
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
        log::debug!("auth resolve provider={} source=override", provider.id);
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
                    log::debug!("auth resolve provider={} source=oauth", provider.id);
                    resolve_stored_oauth(credentials, &provider.id, oauth, cred).await
                } else {
                    log::debug!("auth unresolved provider={} source=oauth_no_handler", provider.id);
                    Ok(None)
                }
            }
            Credential::ApiKey(cred) => {
                log::debug!("auth resolve provider={} source=stored_api_key", provider.id);
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
        let result = resolve_api_key(ctx, api_key, model, None, overrides.and_then(|o| o.env)).await;
        match &result {
            Ok(Some(_)) => log::debug!("auth resolved provider={} source=api_key", provider.id),
            Ok(None) => log::debug!("auth unresolved provider={} source=api_key", provider.id),
            Err(e) => log::warn!("auth resolve failed provider={}: {e}", provider.id),
        }
        return result;
    }

    log::debug!("auth unresolved provider={} source=none", provider.id);
    Ok(None)
}

pub struct ProviderAuthHolder {
    pub id: String,
    pub auth: ProviderAuth,
}

/// True when the OAuth refresh response indicates the refresh token is dead.
fn is_revoked_oauth_refresh_error(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("invalid_grant")
        || lower.contains("revoked")
        || lower.contains("token has been expired")
        || lower.contains("refresh token") && lower.contains("invalid")
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
                let detail = e.to_string();
                // Revoked / expired refresh tokens cannot be recovered — drop the stored
                // credential so bootstrap and later calls do not loop on invalid_grant.
                if is_revoked_oauth_refresh_error(&detail) {
                    let _ = credentials
                        .modify(provider_id, Box::new(move |_| Box::pin(async move { None })))
                        .await;
                    log::warn!(
                        "OAuth refresh token revoked for {provider_id}; cleared in-memory credential. Re-connect the provider."
                    );
                    return Ok(None);
                }
                log::warn!("OAuth refresh failed for {provider_id}: {detail}");
                return Err(ModelsError::with_source(
                    ModelsErrorCode::OAuth,
                    format!("OAuth refresh failed for {provider_id}"),
                    e,
                ));
            }
        };
        let refreshed_for_store = refreshed.clone();
        let post = credentials
            .modify(
                provider_id,
                Box::new(move |current| {
                    let refreshed = refreshed_for_store.clone();
                    Box::pin(async move {
                        // Always write the fresh token when the slot is still OAuth (or empty).
                        // A concurrent refresh may have already updated expires; prefer newer.
                        match current {
                            Some(Credential::OAuth(current))
                                if chrono::Utc::now().timestamp_millis() < current.expires
                                    && current.expires >= refreshed.expires =>
                            {
                                Some(Credential::OAuth(current))
                            }
                            _ => Some(Credential::OAuth(refreshed)),
                        }
                    })
                }),
            )
            .await;

        // Prefer store result; never drop a successful refresh just because modify returned None.
        stored = match post {
            Some(Credential::OAuth(cred)) => cred,
            _ => refreshed,
        };
    }

    match (oauth.to_auth)(stored).await {
        Ok(auth) => Ok(Some(AuthResult {
            auth,
            env: None,
            source: Some("OAuth".to_string()),
        })),
        Err(e) => Err(ModelsError::with_source(
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
