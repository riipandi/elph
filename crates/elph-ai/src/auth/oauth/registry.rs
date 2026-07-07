//! Global OAuth provider registry for elph-ai.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::auth::helpers::lazy_oauth;
use crate::auth::oauth::{anthropic_oauth_loader, github_copilot_oauth_loader, openai_codex_oauth_loader};
use crate::auth::types::{AuthLoginCallbacks, ModelAuth, OAuthAuth, OAuthCredential};
use crate::models::catalog::GITHUB_COPILOT_MODELS;
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
    vec![anthropic_provider(), github_copilot_provider(), openai_codex_provider()]
}

fn modify_github_copilot_models(models: Vec<Model>, credential: &OAuthCredential) -> Vec<Model> {
    let enterprise_domain = credential
        .enterprise_url
        .as_deref()
        .and_then(crate::auth::oauth::normalize_domain);
    let base_url =
        crate::auth::oauth::get_github_copilot_base_url(Some(&credential.access), enterprise_domain.as_deref());
    models
        .into_iter()
        .map(|mut model| {
            model.base_url = base_url.clone();
            model
        })
        .collect()
}

static REGISTRY: once_cell::sync::Lazy<RwLock<HashMap<String, OAuthProviderInterface>>> =
    once_cell::sync::Lazy::new(|| {
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

pub async fn refresh_oauth_token(provider_id: &str, credential: OAuthCredential) -> anyhow::Result<OAuthCredential> {
    let provider =
        get_oauth_provider(provider_id).ok_or_else(|| anyhow::anyhow!("Unknown OAuth provider: {provider_id}"))?;
    (provider.auth.refresh)(credential).await
}

pub struct OAuthApiKeyResult {
    pub new_credentials: OAuthCredential,
    pub api_key: String,
}

pub async fn get_oauth_api_key(
    provider_id: &str,
    mut credential: OAuthCredential,
) -> anyhow::Result<OAuthApiKeyResult> {
    let provider =
        get_oauth_provider(provider_id).ok_or_else(|| anyhow::anyhow!("Unknown OAuth provider: {provider_id}"))?;

    if chrono::Utc::now().timestamp_millis() >= credential.expires {
        credential = (provider.auth.refresh)(credential).await?;
    }

    let api_key = (provider.get_api_key)(&credential);
    Ok(OAuthApiKeyResult {
        new_credentials: credential,
        api_key,
    })
}

pub async fn oauth_provider_login(
    provider_id: &str,
    callbacks: Arc<dyn AuthLoginCallbacks>,
) -> anyhow::Result<OAuthCredential> {
    let provider =
        get_oauth_provider(provider_id).ok_or_else(|| anyhow::anyhow!("Unknown OAuth provider: {provider_id}"))?;
    (provider.auth.login)(callbacks).await
}

pub async fn oauth_provider_to_auth(provider_id: &str, credential: OAuthCredential) -> anyhow::Result<ModelAuth> {
    let provider =
        get_oauth_provider(provider_id).ok_or_else(|| anyhow::anyhow!("Unknown OAuth provider: {provider_id}"))?;
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
    vec!["anthropic", "github-copilot", "openai-codex"]
}

pub fn github_copilot_catalog_models() -> Vec<Model> {
    GITHUB_COPILOT_MODELS.iter().cloned().collect()
}
