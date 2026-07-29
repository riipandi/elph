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
//!
//! // Load all stored provider keys
//! let creds = load_all_provider_credentials(&auth_store_path).await?;
//!
//! // Remove a stored key
//! remove_provider_credential(&auth_store_path, "anthropic").await?;
//! ```

use std::path::Path;
use std::sync::Arc;

use elph_agent::{Aes256Key, decrypt_string_async, default_auth_key_path, encrypt_string_async, is_encrypted_value};
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

    log::info!("Saved encrypted API key for provider: {provider_id}");
    Ok(())
}

/// Load a decrypted API key for a specific provider from `auth.json`.
pub async fn load_provider_credential(auth_store_path: &Path, provider_id: &str) -> anyhow::Result<Option<String>> {
    let file = AuthStoreFile::load_from_path(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("read auth store: {e}"))?;

    let Some(enc) = file.get_provider_credential(provider_id) else {
        return Ok(None);
    };

    if !is_encrypted_value(enc) {
        return Ok(None);
    }

    let key_path = default_auth_key_path(auth_store_path);
    let key = Aes256Key::load_or_create(key_path)
        .await
        .map_err(|e| anyhow::anyhow!("load auth key: {e}"))?;
    let key = Arc::new(key);

    let plaintext = decrypt_string_async(Arc::clone(&key), enc.to_owned())
        .await
        .map_err(|e| anyhow::anyhow!("decrypt API key: {e}"))?;

    Ok(Some(plaintext))
}

/// Load all stored provider credentials from `auth.json`, returning
/// `(provider_id, decrypted_api_key)` pairs.
pub async fn load_all_provider_credentials(auth_store_path: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let file = AuthStoreFile::load_from_path(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("read auth store: {e}"))?;

    let ids: Vec<String> = file.provider_ids();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let key_path = default_auth_key_path(auth_store_path);
    let key = Aes256Key::load_or_create(key_path)
        .await
        .map_err(|e| anyhow::anyhow!("load auth key: {e}"))?;
    let key = Arc::new(key);

    let mut results = Vec::new();
    for provider_id in &ids {
        let Some(enc) = file.get_provider_credential(provider_id) else {
            continue;
        };
        if !is_encrypted_value(enc) {
            continue;
        }
        match decrypt_string_async(Arc::clone(&key), enc.to_owned()).await {
            Ok(plaintext) => results.push((provider_id.clone(), plaintext)),
            Err(e) => {
                log::warn!("Failed to decrypt stored API key for provider {provider_id}: {e}");
            }
        }
    }

    Ok(results)
}

/// Remove a stored provider credential from `auth.json`.
pub async fn remove_provider_credential(auth_store_path: &Path, provider_id: &str) -> anyhow::Result<bool> {
    let _guard = lock_auth_store(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("lock auth store: {e}"))?;

    let mut file = AuthStoreFile::load_from_path(auth_store_path)
        .await
        .map_err(|e| anyhow::anyhow!("read auth store: {e}"))?;

    if file.remove_provider_credential(provider_id) {
        file.save_to_path_unlocked(auth_store_path)
            .await
            .map_err(|e| anyhow::anyhow!("write auth store: {e}"))?;
        log::info!("Removed encrypted API key for provider: {provider_id}");
        Ok(true)
    } else {
        Ok(false)
    }
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
