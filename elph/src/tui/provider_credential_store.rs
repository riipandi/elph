//! Sealed persistence for provider API keys in `auth.json`.
//!
//! The whole store is AES-256-GCM sealed; the master key lives only in the OS
//! keychain (zero-trust). Logical provider values are API key strings or
//! `env:VAR` references inside the sealed payload.

use std::path::Path;

use elph_agent::{AuthStoreFile, ENV_REF_PREFIX, lock_auth_store};
use serde_json;

/// Load auth file, falling back to plain JSON (legacy format) if sealed load fails.
/// Only falls back when the file is NOT a sealed v2 envelope — prevents silent data loss
/// when the sealed file exists but fails to decrypt (e.g. wrong key).
///
/// Supports two plain JSON formats:
/// - `{ "provider": { "id": "cred" } }` — nested (AuthStoreFile shape, camelCase)
/// - `{ "providers": { "id": "cred" } }` — nested (snake_case fallback)
/// - `{ "id": "cred" }` — flat key-value
async fn load_auth_file_with_fallback(path: &Path) -> anyhow::Result<AuthStoreFile> {
    match AuthStoreFile::load_from_path(path).await {
        Ok(f) => Ok(f),
        Err(e) => {
            // If the file looks like a sealed envelope, do NOT fall back — the sealed
            // load failed for a real reason (wrong key, corruption). Falling back to
            // plain JSON parsing would return an empty store and cause data loss when
            // the caller saves.
            if let Ok(content) = tokio::fs::read_to_string(path).await
                && !elph_agent::looks_like_envelope(content.as_bytes())
            {
                log::warn!("auth store sealed load failed ({}): {e}; trying plain JSON", path.display());
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    let mut file = AuthStoreFile::default();
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
                    } else {
                        // Try flat format: { "id": "cred" }
                        if let Some(obj) = json.as_object() {
                            for (pid, val) in obj {
                                if let Some(s) = val.as_str() {
                                    // Skip known meta keys that aren't provider IDs
                                    if pid != "mcp"
                                        && pid != "v"
                                        && pid != "alg"
                                        && pid != "nonce"
                                        && pid != "ciphertext"
                                    {
                                        file.set_provider_credential(pid, s.to_string());
                                    }
                                }
                            }
                        }
                    }
                    log::debug!("Loaded auth store as plain JSON (legacy format)");
                    return Ok(file);
                }
            }
            Err(anyhow::anyhow!("read auth store: {e}"))
        }
    }
}

/// Save a provider API key into the sealed auth store.
pub async fn save_provider_credential(auth_store_path: &Path, provider_id: &str, api_key: &str) -> anyhow::Result<()> {
    let _guard = lock_auth_store(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("lock auth store: {e}"))?;

    let mut file = load_auth_file_with_fallback(auth_store_path).await?;

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

    let mut file = load_auth_file_with_fallback(auth_store_path).await?;

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
            // Fallback: try to read as plain JSON
            if let Ok(content) = std::fs::read_to_string(auth_store_path)
                && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
                && let Some(providers) = json
                    .get("provider")
                    .or_else(|| json.get("providers"))
                    .and_then(|v| v.as_object())
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
        // Fallback: try to read as plain JSON
        if let Ok(content) = std::fs::read_to_string(auth_store_path)
            && let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&content)
            && let Some(providers) = (if json.get("provider").is_some() {
                json.get_mut("provider")
            } else {
                json.get_mut("providers")
            })
            .and_then(|v| v.as_object_mut())
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
    if file.mcp.is_empty() && file.provider.is_empty() {
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
            // Fallback: try to read as plain JSON
            if let Ok(content) = std::fs::read_to_string(auth_store_path)
                && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
                && let Some(providers) = json
                    .get("provider")
                    .or_else(|| json.get("providers"))
                    .and_then(|v| v.as_object())
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
