//! Model and auth resolution.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use elph_ai::auth::InMemoryCredentialStore;
use elph_ai::auth::types::{ApiKeyCredential, Credential, OAuthCredential};
use elph_ai::get_builtin_model;
use elph_ai::types::ProviderEnv;
use elph_ai::{CreateModelsOptions, CredentialStore, Model, Models};

use super::provider::resolve_provider_and_model;
use crate::platform::Settings;

#[derive(Clone)]
pub struct ModelSelection {
    pub provider: String,
    pub model_id: String,
    pub model: Model,
    pub models: Arc<Models>,
    pub display_name: String,
}

pub async fn resolve_model(
    settings: &Settings,
    provider_override: Option<&str>,
    model_override: Option<&str>,
    auth_store_path: Option<&Path>,
) -> Result<ModelSelection> {
    let (provider, model_id) = resolve_provider_and_model(
        provider_override,
        model_override,
        settings.session.provider_id.as_deref(),
        settings.session.model_id.as_deref(),
    )?;

    // Look up under the resolved provider only. Gateway model ids often contain `/`
    // (e.g. `moonshotai/kimi-k3-free`); never re-interpret that as a different provider.
    let model =
        get_builtin_model(&provider, &model_id).with_context(|| format!("Model not found: {provider}/{model_id}"))?;

    let credentials = load_credentials_from_auth_json(auth_store_path).await?;
    let models = elph_ai::builtin_models(Some(CreateModelsOptions {
        credentials: Some(Arc::new(credentials)),
        ..Default::default()
    }))
    .into_arc();
    models
        .get_provider(&provider)
        .with_context(|| format!("Provider not registered in runtime models collection: {provider}"))?;

    // Auth is optional at bootstrap: revoked OAuth should not block the session.
    // First API call will surface a clear re-auth error; clear dead tokens from disk.
    match models.get_auth(&model).await {
        Ok(_) => {}
        Err(e) if matches!(e.code, elph_ai::ModelsErrorCode::OAuth) => {
            log::warn!("OAuth unavailable for {provider}/{model_id}: {e}");
            if let Some(path) = auth_store_path {
                let detail = e.to_string().to_ascii_lowercase();
                if detail.contains("invalid_grant")
                    || detail.contains("revoked")
                    || detail.contains("token has been expired")
                {
                    if let Err(clear_err) =
                        crate::tui::provider_credential_store::delete_provider_credential(path, &provider).await
                    {
                        log::warn!("failed to clear revoked OAuth for {provider}: {clear_err}");
                    } else {
                        log::info!("cleared revoked OAuth credential for {provider}; re-connect to continue");
                    }
                }
            }
        }
        Err(e) => {
            return Err(e).with_context(|| format!("resolve auth for {provider}/{model_id}"));
        }
    }

    let display_name = model.name.clone();
    Ok(ModelSelection {
        provider,
        model_id: model.id.clone(),
        model,
        models,
        display_name,
    })
}

/// Load provider credentials from `auth.json` into an in-memory credential store.
async fn load_credentials_from_auth_json(auth_store_path: Option<&Path>) -> Result<InMemoryCredentialStore> {
    let store = InMemoryCredentialStore::new();
    let Some(path) = auth_store_path else {
        return Ok(store);
    };
    if !path.exists() {
        return Ok(store);
    }

    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    if bytes.is_empty() {
        return Ok(store);
    }

    let file: elph_agent::AuthStoreFile =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;

    let key_path = elph_agent::default_auth_key_path(path);
    if !key_path.exists() {
        // No key = no encrypted entries to decrypt, but env refs can still be loaded.
    }

    for (provider_id, value) in &file.providers {
        let Some(raw) = value.as_str() else {
            continue;
        };

        if raw.starts_with(elph_agent::ENV_REF_PREFIX) && !raw.starts_with(elph_agent::ENC_PREFIX) {
            // env ref entry: store as ApiKeyCredential with env map
            let var_name = &raw[elph_agent::ENV_REF_PREFIX.len()..];
            let mut env = ProviderEnv::new();
            env.insert(var_name.to_string(), var_name.to_string());
            let cred = Credential::ApiKey(ApiKeyCredential {
                kind: "api_key".to_string(),
                key: None,
                env: Some(env),
            });
            store
                .modify(provider_id, Box::new(move |_| Box::pin(async move { Some(cred) })))
                .await;
        } else if raw.starts_with(elph_agent::ENC_PREFIX) {
            // Encrypted entry — attempt to decrypt
            let Some(ref key_path) = (if key_path.exists() {
                Some(key_path.clone())
            } else {
                None
            }) else {
                continue;
            };
            let key = match elph_agent::Aes256Key::load_or_create(key_path).await {
                Ok(k) => Arc::new(k),
                Err(_) => continue,
            };
            let plain = match elph_agent::decrypt_string_async(Arc::clone(&key), raw.to_string()).await {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Heuristic: if it looks like JSON, try OAuth; otherwise treat as API key
            if plain.trim().starts_with('{')
                && let Ok(oauth) = serde_json::from_str::<OAuthCredential>(&plain)
            {
                let cred = Credential::OAuth(oauth);
                store
                    .modify(provider_id, Box::new(move |_| Box::pin(async move { Some(cred) })))
                    .await;
                continue;
            }
            // Treat as raw API key
            let cred = Credential::ApiKey(ApiKeyCredential::new(plain));
            store
                .modify(provider_id, Box::new(move |_| Box::pin(async move { Some(cred) })))
                .await;
        }
    }

    Ok(store)
}
