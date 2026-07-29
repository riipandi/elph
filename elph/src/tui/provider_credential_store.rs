//! Encrypted persistence for provider API keys in `auth.json`.
//!
//! Provider credentials live under the `providers` map in `auth.json`, alongside
//! the `mcp` map used by MCP server OAuth. Each value is an AES-256-GCM
//! ciphertext with the `enc:` prefix.
//!
//! ## Usage
//!
//! ```ignore
//! // Save an API key (encrypts and writes to auth.json)
//! save_provider_credential(&auth_store_path, "anthropic", "sk-ant-…").await?;
//! ```

use std::path::Path;
use std::sync::Arc;

use elph_agent::{Aes256Key, default_auth_key_path, encrypt_string_async, is_encrypted_value};
use elph_agent::{AuthStoreFile, lock_auth_store};

/// Save an encrypted API key for a provider to `auth.json`.
pub async fn save_provider_credential(auth_store_path: &Path, provider_id: &str, api_key: &str) -> anyhow::Result<()> {
    let key_path = default_auth_key_path(auth_store_path);
    let key = Aes256Key::load_or_create(key_path)
        .await
        .map_err(|e| anyhow::anyhow!("load/create auth key: {e}"))?;
    let key = Arc::new(key);

    let enc = encrypt_string_async(Arc::clone(&key), api_key.to_owned())
        .await
        .map_err(|e| anyhow::anyhow!("encrypt API key: {e}"))?;
    debug_assert!(is_encrypted_value(&enc));

    // Lock, read, merge, write
    let _guard = lock_auth_store(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("lock auth store: {e}"))?;

    let mut file = AuthStoreFile::load_from_path(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("read auth store: {e}"))?;

    file.set_provider_credential(provider_id, enc);

    file.save_to_path_unlocked(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("write auth store: {e}"))?;

    log::debug!("Saved encrypted API key for provider: {provider_id}");
    Ok(())
}

/// Check if a provider has a stored credential in `auth.json` (without decrypting).
pub fn has_provider_credential(auth_store_path: &Path, provider_id: &str) -> bool {
    if !auth_store_path.exists() {
        return false;
    }
    let Ok(bytes) = std::fs::read(auth_store_path) else {
        return false;
    };
    let Ok(file) = serde_json::from_slice::<AuthStoreFile>(&bytes) else {
        return false;
    };
    file.get_provider_credential(provider_id).is_some()
}

/// Delete a provider credential from `auth.json`. Returns true if removed.
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
    file.save_to_path_unlocked(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("write auth store: {e}"))?;
    if removed {
        log::debug!("Removed credential for provider: {provider_id}");
    }
    Ok(removed)
}

/// List all provider IDs with stored credentials in `auth.json`.
pub fn list_providers_with_credentials(auth_store_path: &Path) -> Vec<String> {
    if !auth_store_path.exists() {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(auth_store_path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_slice::<AuthStoreFile>(&bytes) else {
        return Vec::new();
    };
    let mut ids = file.provider_ids();
    ids.sort();
    ids
}
