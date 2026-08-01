//! Sealed persistence for provider API keys in `auth.json`.
//!
//! The whole store is AES-256-GCM sealed; the master key lives only in the OS
//! keychain (zero-trust). Logical provider values are API key strings or
//! `env:VAR` references inside the sealed payload.

use std::path::Path;

use elph_agent::{AuthStoreFile, ENV_REF_PREFIX, lock_auth_store};
use serde_json;

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
    // Try loading the encrypted auth store first
    match AuthStoreFile::load_from_path_sync(auth_store_path) {
        Ok(file) => file.get_provider_credential(provider_id).is_some(),
        Err(_) => {
            // Fallback: try to read as plain JSON (for manually created auth.json files)
            if let Ok(content) = std::fs::read_to_string(auth_store_path)
                && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
                && let Some(providers) = json.get("providers").and_then(|v| v.as_object())
            {
                return providers.get(provider_id).is_some();
            }
            false
        }
    }
}

/// Delete a provider credential. Returns true if removed.
pub async fn delete_provider_credential(auth_store_path: &Path, provider_id: &str) -> anyhow::Result<bool> {
    if !has_provider_credential(auth_store_path, provider_id) {
        return Ok(false);
    }

    // Try loading the encrypted auth store first
    let encrypted_load = AuthStoreFile::load_from_path(auth_store_path).await;

    if encrypted_load.is_err() {
        // Fallback: try to read as plain JSON (for manually created auth.json files)
        if let Ok(content) = std::fs::read_to_string(auth_store_path)
            && let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&content)
            && let Some(providers) = json.get_mut("providers").and_then(|v| v.as_object_mut())
        {
            let removed = providers.remove(provider_id).is_some();
            if removed {
                log::debug!("Removed credential for provider (plain JSON): {provider_id}");
                // Write back as plain JSON
                let updated =
                    serde_json::to_string_pretty(&json).map_err(|e| anyhow::anyhow!("serialize auth store: {e}"))?;
                tokio::fs::write(auth_store_path, updated)
                    .await
                    .map_err(|e| anyhow::anyhow!("write auth store: {e}"))?;
                return Ok(true);
            }
        }
        return Ok(false);
    }

    let _guard = lock_auth_store(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("lock auth store: {e}"))?;
    let mut file = encrypted_load.map_err(|e| anyhow::anyhow!("read auth store: {e}"))?;
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
    // Try loading the encrypted auth store first
    let file = match AuthStoreFile::load_from_path_sync(auth_store_path) {
        Ok(f) => f,
        Err(_) => {
            // Fallback: try to read as plain JSON (for manually created auth.json files)
            if let Ok(content) = std::fs::read_to_string(auth_store_path)
                && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
                && let Some(providers) = json.get("providers").and_then(|v| v.as_object())
            {
                let mut ids: Vec<String> = providers.keys().cloned().collect();
                ids.sort();
                return ids;
            }
            return Vec::new();
        }
    };
    let mut ids = file.provider_ids();
    ids.sort();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn list_providers_with_credentials_reads_plain_json() {
        let mut temp_file = NamedTempFile::new().expect("tempfile");
        let json_content = r#"{
            "providers": {
                "kilo": "env:KILO_API_KEY",
                "opencode": "env:OPENCODE_API_KEY",
                "tokenrouter": "env:TOKENROUTER_API_KEY"
            }
        }"#;
        write!(temp_file, "{}", json_content).expect("write");

        let ids = list_providers_with_credentials(temp_file.path());
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"kilo".to_string()));
        assert!(ids.contains(&"opencode".to_string()));
        assert!(ids.contains(&"tokenrouter".to_string()));
    }

    #[test]
    fn has_provider_credential_reads_plain_json() {
        let mut temp_file = NamedTempFile::new().expect("tempfile");
        let json_content = r#"{
            "providers": {
                "kilo": "env:KILO_API_KEY",
                "opencode": "env:OPENCODE_API_KEY"
            }
        }"#;
        write!(temp_file, "{}", json_content).expect("write");

        assert!(has_provider_credential(temp_file.path(), "kilo"));
        assert!(has_provider_credential(temp_file.path(), "opencode"));
        assert!(!has_provider_credential(temp_file.path(), "nonexistent"));
    }

    #[test]
    fn list_providers_with_credentials_with_actual_auth_json() {
        // Test with the actual auth.json structure from the user's setup
        let mut temp_file = NamedTempFile::new().expect("tempfile");
        let json_content = r#"{
            "providers": {
                "kilo": "env:KILO_API_KEY",
                "opencode": "env:OPENCODE_API_KEY",
                "opencode-go": "env:OPENCODE_API_KEY",
                "opengateway": "env:OPENGATEWAY_API_KEY",
                "tokenrouter": "env:TOKENROUTER_API_KEY"
            }
        }"#;
        write!(temp_file, "{}", json_content).expect("write");

        let ids = list_providers_with_credentials(temp_file.path());
        assert_eq!(ids.len(), 5);
        assert!(ids.contains(&"kilo".to_string()));
        assert!(ids.contains(&"opencode".to_string()));
        assert!(ids.contains(&"opencode-go".to_string()));
        assert!(ids.contains(&"opengateway".to_string()));
        assert!(ids.contains(&"tokenrouter".to_string()));
    }
}
