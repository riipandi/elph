//! Global OAuth provider registry for elph-ai.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use crate::auth::helpers::lazy_oauth;
use crate::auth::oauth::openai_codex_oauth_loader;
use crate::auth::oauth::{
    anthropic_oauth_loader, github_copilot_oauth_loader, hyper_oauth_loader, kimi_oauth_loader,
    openrouter_oauth_loader, radius_oauth_loader, xai_oauth_loader,
};
use crate::auth::types::{AuthLoginCallbacks, ModelAuth, OAuthAuth, OAuthCredential};
use crate::models::catalog::builtin_catalog;
use crate::types::Model;

pub type OAuthProviderId = String;

pub type OAuthModifyModelsFn = Arc<dyn Fn(Vec<Model>, &OAuthCredential) -> Vec<Model> + Send + Sync>;

#[derive(Clone)]
pub struct OAuthProviderInterface {
    pub id: OAuthProviderId,
    pub name: String,
    pub auth: OAuthAuth,
    pub get_api_key: Arc<dyn Fn(&OAuthCredential) -> String + Send + Sync>,
    pub modify_models: Option<OAuthModifyModelsFn>,
}

fn anthropic_provider() -> OAuthProviderInterface {
    OAuthProviderInterface {
        id: "anthropic".to_string(),
        name: "Anthropic (Claude Pro/Max)".to_string(),
        auth: lazy_oauth("Anthropic (Claude Pro/Max)", anthropic_oauth_loader()),
        get_api_key: Arc::new(|c| c.access.clone()),
        modify_models: None,
    }
}

fn github_copilot_provider() -> OAuthProviderInterface {
    OAuthProviderInterface {
        id: "github-copilot".to_string(),
        name: "GitHub Copilot".to_string(),
        auth: lazy_oauth("GitHub Copilot", github_copilot_oauth_loader()),
        get_api_key: Arc::new(|c| c.access.clone()),
        modify_models: Some(Arc::new(modify_github_copilot_models)),
    }
}

fn hyper_provider() -> OAuthProviderInterface {
    OAuthProviderInterface {
        id: "hyper".to_string(),
        name: "Charm Hyper".to_string(),
        auth: lazy_oauth("Charm Hyper", hyper_oauth_loader()),
        get_api_key: Arc::new(|c| c.access.clone()),
        modify_models: None,
    }
}

fn openai_codex_provider() -> OAuthProviderInterface {
    OAuthProviderInterface {
        id: "openai-codex".to_string(),
        name: "OpenAI (ChatGPT Plus/Pro)".to_string(),
        auth: lazy_oauth("OpenAI (ChatGPT Plus/Pro)", openai_codex_oauth_loader()),
        get_api_key: Arc::new(|c| c.access.clone()),
        modify_models: None,
    }
}

fn built_in_providers() -> Vec<OAuthProviderInterface> {
    vec![
        anthropic_provider(),
        github_copilot_provider(),
        hyper_provider(),
        openai_codex_provider(),
        xai_provider(),
        kimi_provider(),
        openrouter_provider(),
        radius_provider(),
    ]
}

fn xai_provider() -> OAuthProviderInterface {
    OAuthProviderInterface {
        id: "xai".to_string(),
        name: "xAI (Grok/X subscription)".to_string(),
        auth: lazy_oauth("xAI (Grok/X subscription)", xai_oauth_loader()),
        get_api_key: Arc::new(|c| c.access.clone()),
        modify_models: None,
    }
}

fn kimi_provider() -> OAuthProviderInterface {
    OAuthProviderInterface {
        id: "kimi-coding".to_string(),
        name: "Kimi Code Subscription".to_string(),
        auth: lazy_oauth("Kimi Code Subscription", kimi_oauth_loader()),
        get_api_key: Arc::new(|c| c.access.clone()),
        modify_models: None,
    }
}

fn openrouter_provider() -> OAuthProviderInterface {
    OAuthProviderInterface {
        id: "openrouter".to_string(),
        name: "OpenRouter".to_string(),
        auth: lazy_oauth("OpenRouter", openrouter_oauth_loader()),
        get_api_key: Arc::new(|c| c.access.clone()),
        modify_models: None,
    }
}

fn radius_provider() -> OAuthProviderInterface {
    OAuthProviderInterface {
        id: "radius".to_string(),
        name: "Inflection AI (pi-messages)".to_string(),
        auth: lazy_oauth("Inflection AI (pi-messages)", radius_oauth_loader()),
        get_api_key: Arc::new(|c| c.access.clone()),
        modify_models: None,
    }
}

/// Rewrite Copilot base URLs and drop models not enabled for this account.
///
/// When `available_model_ids` is set (from `GET /models` after login/refresh), only those
/// ids remain. Free/Student plans are auto-selection-only — typically a short list that may
/// include `auto` — so premium catalog rows must not be offered or requested.
fn modify_github_copilot_models(models: Vec<Model>, credential: &OAuthCredential) -> Vec<Model> {
    let enterprise_domain = credential
        .enterprise_url
        .as_deref()
        .and_then(crate::auth::oauth::normalize_domain);
    let base_url =
        crate::auth::oauth::get_github_copilot_base_url(Some(&credential.access), enterprise_domain.as_deref());

    // `None` → older credential / fetch failed: keep catalog (legacy). `Some` → plan-gated set.
    let allowed = credential
        .available_model_ids
        .as_ref()
        .map(|ids| ids.iter().map(|s| s.as_str()).collect::<std::collections::HashSet<_>>());

    let mut filtered: Vec<Model> = models
        .into_iter()
        .filter_map(|mut model| {
            if model.provider != "github-copilot" {
                return Some(model);
            }
            if let Some(ref allowed) = allowed
                && !allowed.contains(model.id.as_str())
            {
                return None;
            }
            model.base_url = base_url.clone();
            Some(model)
        })
        .collect();

    // Free/Student plans are auto-selection-only. If the plan list filtered everything
    // out (or only listed ids outside our catalog), still offer `auto` when present.
    if allowed.is_some()
        && !filtered.iter().any(|m| m.id == "auto")
        && let Some(auto) = github_copilot_catalog_models().into_iter().find(|m| m.id == "auto")
    {
        let mut auto = auto;
        auto.base_url = base_url;
        filtered.insert(0, auto);
    }

    filtered
}

static REGISTRY: LazyLock<RwLock<HashMap<String, OAuthProviderInterface>>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for provider in built_in_providers() {
        map.insert(provider.id.clone(), provider);
    }
    RwLock::new(map)
});

pub fn get_oauth_provider(id: &str) -> Option<OAuthProviderInterface> {
    REGISTRY.read().ok()?.get(id).cloned()
}

pub fn register_oauth_provider(provider: OAuthProviderInterface) {
    if let Ok(mut registry) = REGISTRY.write() {
        registry.insert(provider.id.clone(), provider);
    }
}

pub fn unregister_oauth_provider(id: &str) {
    let Ok(mut registry) = REGISTRY.write() else {
        return;
    };
    if let Some(built_in) = built_in_providers().into_iter().find(|p| p.id == id) {
        registry.insert(id.to_string(), built_in);
        return;
    }
    registry.remove(id);
}

pub fn reset_oauth_providers() {
    if let Ok(mut registry) = REGISTRY.write() {
        registry.clear();
        for provider in built_in_providers() {
            registry.insert(provider.id.clone(), provider);
        }
    }
}

pub fn get_oauth_providers() -> Vec<OAuthProviderInterface> {
    REGISTRY
        .read()
        .map(|registry| registry.values().cloned().collect())
        .unwrap_or_default()
}

#[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.ai.oauth.refresh"))]
pub async fn refresh_oauth_token(
    provider_id: &str,
    credential: OAuthCredential,
) -> Result<OAuthCredential, crate::auth::ModelsError> {
    crate::trace::add_property("provider.id", provider_id);
    let provider = get_oauth_provider(provider_id)
        .ok_or_else(|| crate::auth::ModelsError::oauth(format!("Unknown OAuth provider: {provider_id}")))?;
    log::debug!("oauth refresh start provider={provider_id}");
    match (provider.auth.refresh)(credential).await {
        Ok(next) => {
            log::debug!("oauth refresh ok provider={provider_id}");
            Ok(next)
        }
        Err(error) => {
            log::warn!("oauth refresh failed provider={provider_id}: {error}");
            Err(error)
        }
    }
}

pub struct OAuthApiKeyResult {
    pub new_credentials: OAuthCredential,
    pub api_key: String,
}

pub async fn get_oauth_api_key(
    provider_id: &str,
    mut credential: OAuthCredential,
) -> Result<OAuthApiKeyResult, crate::auth::ModelsError> {
    let provider = get_oauth_provider(provider_id)
        .ok_or_else(|| crate::auth::ModelsError::oauth(format!("Unknown OAuth provider: {provider_id}")))?;

    if chrono::Utc::now().timestamp_millis() >= credential.expires {
        credential = refresh_oauth_token(provider_id, credential).await?;
    }

    let api_key = (provider.get_api_key)(&credential);
    Ok(OAuthApiKeyResult {
        new_credentials: credential,
        api_key,
    })
}

#[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.ai.oauth.login"))]
pub async fn oauth_provider_login(
    provider_id: &str,
    callbacks: Arc<dyn AuthLoginCallbacks>,
    identity: &crate::types::ClientIdentity,
) -> Result<OAuthCredential, crate::auth::ModelsError> {
    crate::trace::add_property("provider.id", provider_id);
    let provider = get_oauth_provider(provider_id)
        .ok_or_else(|| crate::auth::ModelsError::oauth(format!("Unknown OAuth provider: {provider_id}")))?;
    log::info!("oauth login start provider={provider_id}");
    match (provider.auth.login)(callbacks, identity.clone()).await {
        Ok(cred) => {
            log::info!("oauth login ok provider={provider_id}");
            Ok(cred)
        }
        Err(error) => {
            log::warn!("oauth login failed provider={provider_id}: {error}");
            Err(error)
        }
    }
}

pub async fn oauth_provider_to_auth(
    provider_id: &str,
    credential: OAuthCredential,
) -> Result<ModelAuth, crate::auth::ModelsError> {
    let provider = get_oauth_provider(provider_id)
        .ok_or_else(|| crate::auth::ModelsError::oauth(format!("Unknown OAuth provider: {provider_id}")))?;
    (provider.auth.to_auth)(credential).await
}

pub fn oauth_provider_modify_models(provider_id: &str, models: Vec<Model>, credential: &OAuthCredential) -> Vec<Model> {
    let Some(provider) = get_oauth_provider(provider_id) else {
        return models;
    };
    provider
        .modify_models
        .as_ref()
        .map(|modify| modify(models.clone(), credential))
        .unwrap_or(models)
}

pub fn builtin_oauth_provider_ids() -> Vec<&'static str> {
    vec![
        "anthropic",
        "github-copilot",
        "hyper",
        "openai-codex",
        "xai",
        "kimi-coding",
        "openrouter",
        "radius",
    ]
}

pub fn github_copilot_catalog_models() -> Vec<Model> {
    builtin_catalog("github-copilot").as_ref().clone()
}
