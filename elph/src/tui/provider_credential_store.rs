//! Sealed persistence for provider API keys in `auth.json`.
//!
//! The whole store is AES-256-GCM sealed; the master key lives only in the OS
//! keychain (zero-trust). Logical provider values are API key strings or
//! `env:VAR` references inside the sealed payload.

use std::path::Path;

use elph_agent::{AuthStoreFile, ENV_REF_PREFIX, lock_auth_store};

/// Save a provider API key into the sealed auth store.
pub async fn save_provider_credential(auth_store_path: &Path, provider_id: &str, api_key: &str) -> anyhow::Result<()> {
    let _guard = lock_auth_store(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("lock auth store: {e}"))?;

    let mut file = AuthStoreFile::load_from_path(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("read auth store: {e}"))?;

    file.set_provider_credential(provider_id, api_key.to_string());

    file.save_to_path_unlocked(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("write auth store: {e}"))?;

    log::debug!("Saved provider credential (sealed): {provider_id}");
    Ok(())
}

/// Save an env-var reference for a provider (`env:VAR_NAME` inside the sealed payload).
pub async fn save_provider_env_ref(auth_store_path: &Path, provider_id: &str, env_var: &str) -> anyhow::Result<()> {
    let _guard = lock_auth_store(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("lock auth store: {e}"))?;

    let mut file = AuthStoreFile::load_from_path(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("read auth store: {e}"))?;

    file.set_provider_credential(provider_id, format!("{ENV_REF_PREFIX}{env_var}"));

    file.save_to_path_unlocked(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("write auth store: {e}"))?;

    log::debug!("Saved env ref for provider: {provider_id} -> {env_var}");
    Ok(())
}

/// Check if a provider has a stored credential (unseals store).
pub fn has_provider_credential(auth_store_path: &Path, provider_id: &str) -> bool {
    AuthStoreFile::load_from_path_sync(auth_store_path)
        .map(|file| file.get_provider_credential(provider_id).is_some())
        .unwrap_or(false)
}

/// Delete a provider credential. Returns true if removed.
pub async fn delete_provider_credential(auth_store_path: &Path, provider_id: &str) -> anyhow::Result<bool> {
    if !has_provider_credential(auth_store_path, provider_id) {
        return Ok(false);
    }
    let _guard = lock_auth_store(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("lock auth store: {e}"))?;
    let mut file = AuthStoreFile::load_from_path(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("read auth store: {e}"))?;
    let removed = file.remove_provider_credential(provider_id);
    if file.mcp.is_empty() && file.providers.is_empty() {
        if auth_store_path.exists() {
            let _ = tokio::fs::remove_file(auth_store_path).await;
        }
    } else {
        file.save_to_path_unlocked(auth_store_path)
            .await
            .map_err(|e| anyhow::anyhow!("write auth store: {e}"))?;
    }
    if removed {
        log::debug!("Removed credential for provider: {provider_id}");
    }
    Ok(removed)
}

/// List all provider IDs with stored credentials.
pub fn list_providers_with_credentials(auth_store_path: &Path) -> Vec<String> {
    let Ok(file) = AuthStoreFile::load_from_path_sync(auth_store_path) else {
        return Vec::new();
    };
    let mut ids = file.provider_ids();
    ids.sort();
    ids
}
