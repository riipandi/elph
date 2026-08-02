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
use super::provider_catalog::install_providers_dir;
use crate::platform::Settings;
use crate::utils::path::AppPaths;

#[derive(Clone)]
pub struct ModelSelection {
    pub provider: String,
    pub model_id: String,
    pub model: Model,
    pub models: Arc<Models>,
    pub display_name: String,
}

pub(crate) fn selection_from_model(model: &Model, models: Arc<Models>) -> ModelSelection {
    let provider = model.provider.clone();
    let model_id = model.id.clone();
    let display_name = model.name.clone();
    ModelSelection {
        provider: provider.clone(),
        model_id: model_id.clone(),
        model: model.clone(),
        models,
        display_name,
    }
}

pub async fn resolve_model(
    settings: &Settings,
    provider_override: Option<&str>,
    model_override: Option<&str>,
    auth_store_path: Option<&Path>,
) -> Result<(ModelSelection, elph_ai::OverlayApplyReport)> {
    // Prefer CONFIG_DIR/providers when available (resolved via auth store path parent).
    if let Some(auth_path) = auth_store_path {
        if let Some(config_dir) = auth_path.parent() {
            let providers_dir = config_dir.join("providers");
            let _ = install_providers_dir(&providers_dir);
        }
    } else if let Ok(paths) = crate::platform::Paths::resolve() {
        let _ = install_providers_dir(&paths.providers_dir());
    }

    let (default_provider, default_model_id) = match settings.models.default_provider_and_model() {
        Some((p, m)) => (Some(p), Some(m)),
        None => (None, None),
    };
    let (provider, model_id) = resolve_provider_and_model(
        provider_override,
        model_override,
        default_provider.as_deref(),
        default_model_id.as_deref(),
    )?;

    // Look up under the resolved provider only. Gateway model ids often contain `/`
    // (e.g. `moonshotai/kimi-k3-free`); never re-interpret that as a different provider.
    // Honors disk overrides via set_disk_catalog_overrides.
    let model =
        get_builtin_model(&provider, &model_id).with_context(|| format!("Model not found: {provider}/{model_id}"))?;

    let credentials = load_credentials_from_auth_json(auth_store_path).await?;
    let mut mutable = elph_ai::builtin_models(Some(CreateModelsOptions {
        credentials: Some(Arc::new(credentials)),
        ..Default::default()
    }));
    // Merge disk overlays and register streaming adapters for disk-only provider ids.
    let overlays = elph_ai::disk_catalog_overrides();
    let overlay_stats = mutable.apply_model_overlays(&overlays);
    let models = mutable.into_arc();
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
    Ok((
        ModelSelection {
            provider,
            model_id: model.id.clone(),
            model,
            models,
            display_name,
        },
        overlay_stats,
    ))
}

/// Try to load a plain JSON auth file (legacy format without sealed envelope).
///
/// Returns `Some(AuthStoreFile)` with the providers parsed from the JSON,
/// or `None` if the file is not valid plain JSON.
/// Returns `None` without attempting when the file looks like a sealed v2 envelope.
///
/// Supports formats:
/// - `{ "provider": { "id": "cred" } }` — nested (camelCase, AuthStoreFile shape)
/// - `{ "providers": { "id": "cred" } }` — nested (snake_case fallback)
/// - `{ "id": "cred" }` — flat key-value
async fn try_load_plain_json_auth(path: &Path) -> Option<elph_agent::AuthStoreFile> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    // If it looks like a sealed envelope, do not attempt plain JSON parsing —
    // the sealed load failed for a real reason and the envelope has no plaintext providers.
    if elph_agent::looks_like_envelope(content.as_bytes()) {
        return None;
    }
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let mut file = elph_agent::AuthStoreFile::default();
    // Try nested format: "provider" (camelCase) or "providers" (snake_case)
    let providers_obj = json
        .get("provider")
        .or_else(|| json.get("providers"))
        .and_then(|v| v.as_object());
    if let Some(providers) = providers_obj {
        for (pid, val) in providers {
            if let Some(s) = val.as_str() {
                file.set_provider_credential(pid, s.to_string());
            }
        }
    } else if let Some(obj) = json.as_object() {
        // Try flat format: { "id": "cred" }
        for (pid, val) in obj {
            if let Some(s) = val.as_str()
                && pid != "mcp"
                && pid != "v"
                && pid != "alg"
                && pid != "nonce"
                && pid != "ciphertext"
            {
                file.set_provider_credential(pid, s.to_string());
            }
        }
    }
    if file.provider.is_empty() { None } else { Some(file) }
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

    let file = match elph_agent::AuthStoreFile::load_from_path(path).await {
        Ok(f) => f,
        Err(e) => {
            log::warn!("auth store load failed ({}): {e}", path.display());
            // Fallback: try plain JSON (legacy format without sealed envelope)
            match try_load_plain_json_auth(path).await {
                Some(f) => f,
                None => return Ok(store),
            }
        }
    };

    for (provider_id, value) in &file.provider {
        let Some(raw) = value.as_str() else {
            continue;
        };

        if let Some(var_name) = raw.strip_prefix(elph_agent::ENV_REF_PREFIX) {
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
            continue;
        }

        // Sealed payload stores API keys (or JSON OAuth blobs) as plain strings in-memory.
        if raw.trim().starts_with('{')
            && let Ok(oauth) = serde_json::from_str::<OAuthCredential>(raw)
        {
            let cred = Credential::OAuth(oauth);
            store
                .modify(provider_id, Box::new(move |_| Box::pin(async move { Some(cred) })))
                .await;
            continue;
        }
        let plain = raw.to_string();
        let cred = Credential::ApiKey(ApiKeyCredential::new(plain));
        store
            .modify(provider_id, Box::new(move |_| Box::pin(async move { Some(cred) })))
            .await;
    }

    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_from_model_preserves_model_identity() {
        let model = get_builtin_model("openai", "gpt-5.6-luna").expect("builtin model should exist");
        let models = elph_ai::builtin_models(None).into_arc();
        let selection = selection_from_model(&model, models);

        assert_eq!(selection.provider, model.provider);
        assert_eq!(selection.model_id, model.id);
        assert_eq!(selection.display_name, model.name);
        assert_eq!(selection.model.id, model.id);
    }
}
