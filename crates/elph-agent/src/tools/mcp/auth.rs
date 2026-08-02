//! OAuth 2.1 credential storage and authorization helpers for remote MCP servers.
//!
//! Credentials live in a plain JSON file (default name [`DEFAULT_AUTH_FILE_NAME`] =
//! `auth.json`) where individual string values are encrypted with an `enc:` prefix
//! (AES-256-GCM). The master key is kept only in the OS keychain (zero-trust) —
//! never as `auth.key` beside the store.
//!
//! On-disk format:
//! ```json
//! { "mcp": { "<server>": "enc:…" | "env:VAR" }, "provider": { "<id>": "enc:…" | "env:VAR" } }
//! ```
//!
//! `env:` references are stored in plaintext — they are not secrets, only references
//! to environment variables. Every other string value is encrypted at the field level.
//!
//! The path is **not** hardcoded — each host passes it via [`AuthStorePathBuilder`] /
//! [`McpLoadOptions::auth_store_path`](super::config::McpLoadOptions).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rmcp::transport::auth::{
    AuthError, AuthorizationManager, AuthorizationRequest, AuthorizationSession, CredentialStore, StoredCredentials,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use super::crypto::Aes256Key;
use super::crypto::{decrypt_string_sync, encrypt_string_sync, is_encrypted_value};
use super::key_provider::load_or_create_master_key;
use super::store_lock::{atomic_write_private, lock_auth_store};

/// Default OAuth scopes when the server does not advertise any.
pub const DEFAULT_OAUTH_SCOPES: &[&str] = &[];

/// Default credential store filename (joined under a host-provided config dir).
pub const DEFAULT_AUTH_FILE_NAME: &str = "auth.json";

// ---------------------------------------------------------------------------
// Path resolution (host-agnostic)
// ---------------------------------------------------------------------------

/// Builds a filesystem path for the shared auth store file.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use elph_agent::AuthStorePathBuilder;
///
/// let path = AuthStorePathBuilder::new()
///     .base_dir("/home/user/.elph")
///     .build();
/// assert_eq!(path, PathBuf::from("/home/user/.elph/auth.json"));
/// ```
#[derive(Debug, Clone)]
pub struct AuthStorePathBuilder {
    base_dir: Option<PathBuf>,
    file_name: String,
    path: Option<PathBuf>,
}

impl Default for AuthStorePathBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthStorePathBuilder {
    pub fn new() -> Self {
        Self {
            base_dir: None,
            file_name: DEFAULT_AUTH_FILE_NAME.to_string(),
            path: None,
        }
    }

    pub fn base_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = name.into();
        self
    }

    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn build(self) -> PathBuf {
        if let Some(path) = self.path {
            return path;
        }
        if let Some(base) = self.base_dir {
            return base.join(self.file_name);
        }
        PathBuf::from(self.file_name)
    }
}

/// Convenience: `config_dir/auth.json` using the default filename.
pub fn auth_store_path(config_dir: &Path) -> PathBuf {
    AuthStorePathBuilder::new().base_dir(config_dir).build()
}

// ---------------------------------------------------------------------------
// On-disk format (multi-server, encrypted entries)
// ---------------------------------------------------------------------------

/// Prefix for env-var reference entries stored in the providers map.
///
/// An entry like `"openai": "env:OPENAI_API_KEY"` means "read the credential
/// from the `OPENAI_API_KEY` environment variable." These entries are **not**
/// encrypted — they are references, not secrets.
pub const ENV_REF_PREFIX: &str = "env:";

/// Logical auth store document (plaintext secrets only while in memory).
///
/// On disk, individual string values are encrypted with `enc:` prefix (AES-256-GCM)
/// while `env:` references are stored as-is. The whole file is valid JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStoreFile {
    /// Map of MCP server name → OAuth credential JSON object (or null).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp: BTreeMap<String, Value>,
    /// Map of provider ID → API key string or `env:VAR` reference.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider: BTreeMap<String, Value>,
}

impl AuthStoreFile {
    /// Load and decrypt the auth store (OS keychain master key). Missing / empty → empty store.
    ///
    /// Reads plain JSON with per-field `enc:` values, decrypts them, returns plaintext in memory.
    pub async fn load_from_path(path: &Path) -> Result<Self, AuthError> {
        let key = load_or_create_master_key().map_err(|e| AuthError::InternalError(format!("auth master key: {e}")))?;
        Self::load_from_path_with_key(path, &key).await
    }

    /// Load with an explicit master key (tests / injectors).
    pub async fn load_from_path_with_key(path: &Path, key: &Aes256Key) -> Result<Self, AuthError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| AuthError::InternalError(format!("read auth store: {e}")))?;
        Self::from_plain_json_bytes(&bytes, key)
    }

    /// Sync load (CLI probes) using the OS keychain master key.
    pub fn load_from_path_sync(path: &Path) -> Result<Self, AuthError> {
        let key = load_or_create_master_key().map_err(|e| AuthError::InternalError(format!("auth master key: {e}")))?;
        Self::load_from_path_sync_with_key(path, &key)
    }

    /// Sync load with an explicit master key.
    pub fn load_from_path_sync_with_key(path: &Path, key: &Aes256Key) -> Result<Self, AuthError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(path).map_err(|e| AuthError::InternalError(format!("read auth store: {e}")))?;
        Self::from_plain_json_bytes(&bytes, key)
    }

    /// Parse plain JSON bytes, decrypting any `enc:` values.
    fn from_plain_json_bytes(bytes: &[u8], key: &Aes256Key) -> Result<Self, AuthError> {
        if bytes.is_empty() {
            return Ok(Self::default());
        }

        // Check if the file is a sealed v2 envelope (legacy format from before the
        // per-field enc: migration). If so, unseal it and parse the inner JSON.
        if let Ok(top) = serde_json::from_slice::<serde_json::Value>(bytes) {
            if top.get("v").and_then(|x| x.as_u64()) == Some(2)
                && top.get("ciphertext").is_some()
                && top.get("nonce").is_some()
            {
                let envelope: super::envelope::AuthStoreEnvelope = serde_json::from_slice(bytes)
                    .map_err(|e| AuthError::InternalError(format!("parse auth envelope: {e}")))?;
                let plain = super::envelope::unseal_store(key, &envelope)
                    .map_err(|e| AuthError::InternalError(format!("unseal legacy auth store: {e}")))?;
                let mut json: serde_json::Value = serde_json::from_slice(&plain)
                    .map_err(|e| AuthError::InternalError(format!("parse unsealed auth payload: {e}")))?;
                // Normalize "providers" (plural, legacy) to "provider" (singular, camelCase)
                if json.get("providers").is_some() && json.get("provider").is_none() {
                    if let Some(obj) = json.as_object_mut() {
                        if let Some(v) = obj.remove("providers") {
                            obj.insert("provider".to_string(), v);
                        }
                    }
                }
                return serde_json::from_value(json)
                    .map_err(|e| AuthError::InternalError(format!("parse unsealed auth payload: {e}")));
            }
        }

        let mut json: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| AuthError::InternalError(format!("parse auth JSON: {e}")))?;

        // Normalize "providers" (plural, legacy) to "provider" (singular, camelCase)
        // so serde rename_all = "camelCase" on AuthStoreFile can deserialize it.
        if json.get("providers").is_some() && json.get("provider").is_none() {
            if let Some(obj) = json.as_object_mut() {
                if let Some(v) = obj.remove("providers") {
                    obj.insert("provider".to_string(), v);
                }
            }
        }

        // Decrypt provider values (serialized as "provider" due to rename_all = "camelCase")
        if let Some(providers) = json.get_mut("provider").and_then(|v| v.as_object_mut()) {
            for (_, val) in providers.iter_mut() {
                if let Some(s) = val.as_str() {
                    if let Ok(plain) = decrypt_string_sync(key, s) {
                        *val = Value::String(plain);
                    }
                    // other prefixes (env:, plain) are left as-is
                }
            }
        }

        // Decrypt mcp values (JSON objects were serialized to string then encrypted)
        if let Some(mcp) = json.get_mut("mcp").and_then(|v| v.as_object_mut()) {
            for (_, val) in mcp.iter_mut() {
                if let Some(s) = val.as_str() {
                    if let Ok(plain) = decrypt_string_sync(key, s) {
                        // Try to parse as JSON object (StoredCredentials)
                        if let Ok(obj) = serde_json::from_str::<Value>(&plain) {
                            *val = obj;
                        } else {
                            *val = Value::String(plain);
                        }
                    }
                    // other prefixes (env:, plain) are left as-is
                }
            }
        }

        serde_json::from_value(json).map_err(|e| AuthError::InternalError(format!("parse auth payload: {e}")))
    }

    /// Encrypt and write without taking the store lock (caller must hold [`lock_auth_store`]).
    ///
    /// Writes plain JSON with per-field `enc:` encryption. `env:` references are
    /// stored as-is (they are not secrets).
    pub async fn save_to_path_unlocked(&self, path: &Path) -> Result<(), AuthError> {
        let key = load_or_create_master_key().map_err(|e| AuthError::InternalError(format!("auth master key: {e}")))?;
        self.save_to_path_unlocked_with_key(path, &key).await
    }

    /// Encrypt and write with an explicit master key.
    pub async fn save_to_path_unlocked_with_key(&self, path: &Path, key: &Aes256Key) -> Result<(), AuthError> {
        // Serialize self to JSON Value
        let mut json =
            serde_json::to_value(self).map_err(|e| AuthError::InternalError(format!("serialize auth payload: {e}")))?;

        // Encrypt non-env provider values
        if let Some(providers) = json.get_mut("provider").and_then(|v| v.as_object_mut()) {
            for (_, val) in providers.iter_mut() {
                if let Some(s) = val.as_str() {
                    if !s.starts_with(ENV_REF_PREFIX) && !is_encrypted_value(s) {
                        let encrypted = encrypt_string_sync(key, s)
                            .map_err(|e| AuthError::InternalError(format!("encrypt provider value: {e}")))?;
                        *val = Value::String(encrypted);
                    }
                }
            }
        }

        // Encrypt non-env mcp values (JSON objects get serialized then encrypted)
        if let Some(mcp) = json.get_mut("mcp").and_then(|v| v.as_object_mut()) {
            for (_, val) in mcp.iter_mut() {
                match val {
                    Value::Object(_) => {
                        // Serialize object to JSON string, then encrypt
                        let obj_str = serde_json::to_string(val)
                            .map_err(|e| AuthError::InternalError(format!("serialize mcp value: {e}")))?;
                        let encrypted = encrypt_string_sync(key, &obj_str)
                            .map_err(|e| AuthError::InternalError(format!("encrypt mcp value: {e}")))?;
                        *val = Value::String(encrypted);
                    }
                    Value::String(s) if !s.starts_with(ENV_REF_PREFIX) && !is_encrypted_value(s) => {
                        let encrypted = encrypt_string_sync(key, s)
                            .map_err(|e| AuthError::InternalError(format!("encrypt mcp value: {e}")))?;
                        *val = Value::String(encrypted);
                    }
                    _ => {}
                }
            }
        }

        let bytes = serde_json::to_vec_pretty(&json)
            .map_err(|e| AuthError::InternalError(format!("serialize auth JSON: {e}")))?;
        atomic_write_private(path, &bytes)
            .await
            .map_err(|e| AuthError::InternalError(e.to_string()))?;
        Ok(())
    }

    /// Lock the store, then seal + atomic-write.
    pub async fn save_to_path(&self, path: &Path) -> Result<(), AuthError> {
        let _guard = lock_auth_store(path)
            .await
            .map_err(|e| AuthError::InternalError(format!("lock auth store: {e}")))?;
        self.save_to_path_unlocked(path).await
    }

    /// Lock + save with an explicit master key (tests).
    pub async fn save_to_path_with_key(&self, path: &Path, key: &Aes256Key) -> Result<(), AuthError> {
        let _guard = lock_auth_store(path)
            .await
            .map_err(|e| AuthError::InternalError(format!("lock auth store: {e}")))?;
        self.save_to_path_unlocked_with_key(path, key).await
    }

    pub fn contains_server(&self, server_name: &str) -> bool {
        self.mcp.contains_key(server_name)
    }

    /// Set provider credential (API key plaintext or `env:VAR`). Caller must hold the lock.
    /// Secrets are encrypted at the field level when written to disk.
    pub fn set_provider_credential(&mut self, provider_id: &str, credential: String) {
        self.provider
            .insert(provider_id.to_string(), Value::String(credential));
    }

    /// Get provider credential string (API key or `env:VAR`).
    pub fn get_provider_credential(&self, provider_id: &str) -> Option<&str> {
        self.provider.get(provider_id).and_then(|v| v.as_str())
    }

    /// Remove a provider credential. Returns `true` if it existed.
    pub fn remove_provider_credential(&mut self, provider_id: &str) -> bool {
        self.provider.remove(provider_id).is_some()
    }

    /// List all provider IDs that have stored credentials.
    pub fn provider_ids(&self) -> Vec<String> {
        self.provider.keys().cloned().collect()
    }

    /// Check if a provider entry is an env-var reference (`env:VAR_NAME`).
    pub fn is_env_ref(&self, provider_id: &str) -> bool {
        self.provider
            .get(provider_id)
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.starts_with(ENV_REF_PREFIX))
    }

    /// Extract the env var name from an `env:…` entry, e.g. `"env:OPENAI_API_KEY"` → `"OPENAI_API_KEY"`.
    pub fn env_var_name(&self, provider_id: &str) -> Option<String> {
        self.provider
            .get(provider_id)
            .and_then(|v| v.as_str())
            .filter(|s| s.starts_with(ENV_REF_PREFIX))
            .map(|s| s[ENV_REF_PREFIX.len()..].to_string())
    }
}

// ---------------------------------------------------------------------------
// Per-server CredentialStore backed by shared encrypted auth.json
// ---------------------------------------------------------------------------

/// File-backed [`CredentialStore`] for **one** MCP server key inside an encrypted `auth.json`.
///
/// The file stores plain JSON with per-field `enc:` encryption; MCP credentials are
/// JSON objects encrypted as strings inside the payload.
#[derive(Clone)]
pub struct FileCredentialStore {
    path: PathBuf,
    server_key: String,
    /// When set, used instead of the OS keychain (tests / injectors).
    master_key: Option<Arc<Aes256Key>>,
    cache: Arc<RwLock<Option<StoredCredentials>>>,
}

impl FileCredentialStore {
    /// Create a store for `server_key` inside the shared encrypted file at `path`.
    pub fn new(path: impl Into<PathBuf>, server_key: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            server_key: server_key.into(),
            master_key: None,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Use an explicit master key (tests). Does not touch the OS keychain.
    pub fn with_key(path: impl Into<PathBuf>, server_key: impl Into<String>, key: Aes256Key) -> Self {
        Self {
            path: path.into(),
            server_key: server_key.into(),
            master_key: Some(Arc::new(key)),
            cache: Arc::new(RwLock::new(None)),
        }
    }

    pub fn builder() -> FileCredentialStoreBuilder {
        FileCredentialStoreBuilder::new()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn server_key(&self) -> &str {
        &self.server_key
    }

    fn resolve_key(&self) -> Result<Aes256Key, AuthError> {
        if let Some(k) = &self.master_key {
            return Ok(Aes256Key::from_bytes(*k.as_bytes()));
        }
        load_or_create_master_key().map_err(|e| AuthError::InternalError(format!("auth master key: {e}")))
    }

    async fn load_entry(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let _guard = lock_auth_store(&self.path)
            .await
            .map_err(|e| AuthError::InternalError(format!("lock auth store: {e}")))?;
        let key = self.resolve_key()?;
        let file = AuthStoreFile::load_from_path_with_key(&self.path, &key).await?;
        let Some(value) = file.mcp.get(&self.server_key) else {
            return Ok(None);
        };
        decode_entry(value)
    }

    /// Load-merge-save under exclusive lock (safe for concurrent token refresh).
    async fn write_entry(&self, credentials: Option<StoredCredentials>) -> Result<(), AuthError> {
        let _guard = lock_auth_store(&self.path)
            .await
            .map_err(|e| AuthError::InternalError(format!("lock auth store: {e}")))?;
        let key = self.resolve_key()?;
        let mut file = AuthStoreFile::load_from_path_with_key(&self.path, &key).await?;
        match credentials {
            Some(creds) => {
                let value = serde_json::to_value(&creds)
                    .map_err(|e| AuthError::InternalError(format!("serialize credentials: {e}")))?;
                file.mcp.insert(self.server_key.clone(), value);
            }
            None => {
                file.mcp.remove(&self.server_key);
            }
        }
        file.save_to_path_unlocked_with_key(&self.path, &key).await?;
        Ok(())
    }
}

fn decode_entry(value: &Value) -> Result<Option<StoredCredentials>, AuthError> {
    match value {
        Value::Null => Ok(None),
        Value::Object(_) => {
            let creds: StoredCredentials = serde_json::from_value(value.clone())
                .map_err(|e| AuthError::InternalError(format!("parse MCP credentials: {e}")))?;
            Ok(Some(creds))
        }
        other => Err(AuthError::InternalError(format!(
            "unexpected MCP credential entry type (expected JSON object): {other}"
        ))),
    }
}

/// Builder for [`FileCredentialStore`].
#[derive(Debug, Clone, Default)]
pub struct FileCredentialStoreBuilder {
    path_builder: AuthStorePathBuilder,
    server_key: Option<String>,
    key: Option<Aes256Key>,
}

impl FileCredentialStoreBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn base_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.path_builder = self.path_builder.base_dir(dir);
        self
    }

    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.path_builder = self.path_builder.file_name(name);
        self
    }

    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path_builder = self.path_builder.path(path);
        self
    }

    pub fn server_key(mut self, key: impl Into<String>) -> Self {
        self.server_key = Some(key.into());
        self
    }

    /// Explicit AES-256 master key (tests / hosts that inject key material).
    pub fn encryption_key(mut self, key: Aes256Key) -> Self {
        self.key = Some(key);
        self
    }

    pub fn build(self) -> Result<FileCredentialStore> {
        let server_key = self
            .server_key
            .filter(|s| !s.trim().is_empty())
            .context("FileCredentialStore requires a non-empty server_key")?;
        let path = self.path_builder.build();
        if let Some(key) = self.key {
            return Ok(FileCredentialStore::with_key(path, server_key, key));
        }
        Ok(FileCredentialStore::new(path, server_key))
    }
}

#[async_trait]
impl CredentialStore for FileCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        {
            let cache = self.cache.read().await;
            if cache.is_some() {
                return Ok(cache.clone());
            }
        }
        let loaded = self.load_entry().await?;
        *self.cache.write().await = loaded.clone();
        Ok(loaded)
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        self.write_entry(Some(credentials.clone())).await?;
        *self.cache.write().await = Some(credentials);
        Ok(())
    }

    async fn clear(&self) -> Result<(), AuthError> {
        *self.cache.write().await = None;
        let _guard = lock_auth_store(&self.path)
            .await
            .map_err(|e| AuthError::InternalError(format!("lock auth store: {e}")))?;
        let key = self.resolve_key()?;
        let mut file = AuthStoreFile::load_from_path_with_key(&self.path, &key).await?;
        file.mcp.remove(&self.server_key);
        if file.mcp.is_empty() && file.provider.is_empty() {
            if self.path.exists() {
                let _ = tokio::fs::remove_file(&self.path).await;
            }
        } else {
            file.save_to_path_unlocked_with_key(&self.path, &key).await?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// True when encrypted `auth.json` contains an entry for `server_name`.
pub fn has_stored_credentials(auth_store_path: &Path, server_name: &str) -> bool {
    AuthStoreFile::load_from_path_sync(auth_store_path)
        .map(|file| file.contains_server(server_name))
        .unwrap_or(false)
}

#[cfg(test)]
mod sealed_store_tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn plain_json_encrypted_roundtrip_no_lock_sidecar() {
        let key = Aes256Key::generate();
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");

        let mut file = AuthStoreFile::default();
        file.set_provider_credential("opencode", "sk-test-secret".into());
        file.save_to_path_with_key(&path, &key).await.unwrap();

        // No lock sidecar or separate key file
        let mut lock_sidecar = path.as_os_str().to_os_string();
        lock_sidecar.push(".lock");
        assert!(!std::path::PathBuf::from(lock_sidecar).exists());
        assert!(!path.with_extension("key").exists());

        // File is plain JSON, not an envelope
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("\"v\": 2"), "should not be an envelope: {raw}");
        assert!(!raw.contains("sk-test-secret"), "plaintext must not appear: {raw}");
        assert!(raw.contains("enc:"), "value should be encrypted with enc: prefix: {raw}");

        let loaded = AuthStoreFile::load_from_path_with_key(&path, &key).await.unwrap();
        assert_eq!(loaded.get_provider_credential("opencode"), Some("sk-test-secret"));
    }

    #[tokio::test]
    async fn env_ref_stored_as_plaintext() {
        let key = Aes256Key::generate();
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");

        let mut file = AuthStoreFile::default();
        file.set_provider_credential("openai", "env:OPENAI_API_KEY".into());
        file.save_to_path_with_key(&path, &key).await.unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("env:OPENAI_API_KEY"), "env ref must be in plaintext: {raw}");
        assert!(!raw.contains("enc:"), "env refs should not be encrypted");

        let loaded = AuthStoreFile::load_from_path_with_key(&path, &key).await.unwrap();
        assert_eq!(loaded.get_provider_credential("openai"), Some("env:OPENAI_API_KEY"));
    }

    #[tokio::test]
    async fn mcp_credentials_encrypted_roundtrip() {
        let key = Aes256Key::generate();
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");

        let mut file = AuthStoreFile::default();
        // StoredCredentials-like JSON object
        let creds = serde_json::json!({
            "clientId": "client-abc",
            "scopes": ["read", "write"],
        });
        file.mcp.insert("server-foo".into(), creds);
        file.save_to_path_with_key(&path, &key).await.unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("client-abc"), "client ID must not appear in plaintext: {raw}");
        assert!(raw.contains("enc:"), "mcp value should be encrypted: {raw}");

        let loaded = AuthStoreFile::load_from_path_with_key(&path, &key).await.unwrap();
        let loaded_val = loaded.mcp.get("server-foo").unwrap();
        assert_eq!(loaded_val.get("clientId").and_then(|v| v.as_str()), Some("client-abc"));
    }

    #[tokio::test]
    async fn loads_plain_legacy_store_with_env_refs() {
        // Plain JSON with only env: refs should load successfully (no encryption needed).
        let key = Aes256Key::generate();
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let content = r#"{"providers":{"openai":"env:OPENAI_API_KEY"}}"#;
        std::fs::write(&path, content).unwrap();

        let loaded = AuthStoreFile::load_from_path_with_key(&path, &key).await.unwrap();
        assert_eq!(loaded.get_provider_credential("openai"), Some("env:OPENAI_API_KEY"));
    }

    #[tokio::test]
    async fn loads_plain_legacy_store_with_mixed() {
        // Plain JSON with mixed env: and plain values — plain values should survive.
        let key = Aes256Key::generate();
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let content = r#"{"providers":{"openai":"env:OPENAI_API_KEY","other":"plain-text-key"}}"#;
        std::fs::write(&path, content).unwrap();

        let loaded = AuthStoreFile::load_from_path_with_key(&path, &key).await.unwrap();
        assert_eq!(loaded.get_provider_credential("openai"), Some("env:OPENAI_API_KEY"));
        assert_eq!(loaded.get_provider_credential("other"), Some("plain-text-key"));
    }
}

/// Remove stored OAuth credentials for a server from the shared store.
pub async fn clear_credentials(auth_store_path: &Path, server_name: &str) -> Result<bool> {
    if !has_stored_credentials(auth_store_path, server_name) {
        return Ok(false);
    }
    let store = FileCredentialStore::new(auth_store_path, server_name);
    store
        .clear()
        .await
        .map_err(|e| anyhow::anyhow!("clear credentials: {e}"))?;
    Ok(true)
}

/// Result of an interactive OAuth authorization flow.
#[derive(Debug)]
pub struct McpOAuthFlowResult {
    pub server_name: String,
    pub credentials_path: PathBuf,
    pub client_id: String,
}

/// Options for [`run_oauth_flow`] (scopes, client metadata, redirect).
#[derive(Debug, Clone, Default)]
pub struct McpOAuthFlowOptions {
    pub scopes: Vec<String>,
    pub client_name: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub client_metadata_url: Option<String>,
    pub redirect_port: Option<u16>,
    pub open_browser: bool,
}

impl McpOAuthFlowOptions {
    pub fn from_server_meta(meta: &super::config::McpOAuthClientMeta) -> Self {
        Self {
            scopes: meta.scopes.clone(),
            client_name: meta.client_name.clone(),
            client_id: meta.client_id.clone(),
            client_secret: meta.client_secret.clone(),
            client_metadata_url: meta.client_metadata_url.clone(),
            redirect_port: meta.redirect_port,
            open_browser: true,
        }
    }

    pub fn with_scopes_override(mut self, scopes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let list: Vec<String> = scopes.into_iter().map(Into::into).collect();
        if !list.is_empty() {
            self.scopes = list;
        }
        self
    }
}

/// Run the OAuth 2.1 authorization-code + PKCE flow for an MCP HTTP/SSE server URL.
pub async fn run_oauth_flow(
    server_name: &str,
    server_url: &str,
    auth_store_path: &Path,
    options: McpOAuthFlowOptions,
) -> Result<McpOAuthFlowResult> {
    let store = FileCredentialStore::new(auth_store_path, server_name);

    let mut manager = AuthorizationManager::new(server_url)
        .await
        .map_err(|e| anyhow::anyhow!("init OAuth manager: {e}"))?;
    manager.set_credential_store(store);

    // Discover AS metadata (RFC 9728 protected-resource → AS metadata, with legacy fallback).
    // rmcp does **not** install the result automatically — callers must `set_metadata`.
    let metadata_resolution = manager
        .resolve_metadata()
        .await
        .map_err(|e| anyhow::anyhow!("resolve OAuth metadata: {e}"))?;
    log::info!(
        "MCP OAuth metadata resolved: server={server_name} source={:?} registration={}",
        metadata_resolution.source,
        metadata_resolution
            .metadata
            .registration_endpoint
            .as_deref()
            .unwrap_or("(none)"),
    );
    manager.set_metadata(metadata_resolution.metadata);

    let bind_addr = match options.redirect_port {
        Some(port) => format!("127.0.0.1:{port}"),
        None => "127.0.0.1:0".to_string(),
    };
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind OAuth callback listener on {bind_addr}"))?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    // Prefer explicit config scopes; otherwise seed from AS/resource discovery (e.g. Figma `mcp:connect`).
    let scopes = if options.scopes.is_empty() {
        manager.select_scopes(None, &[])
    } else {
        options.scopes.clone()
    };

    let mut auth_request = AuthorizationRequest::new(&redirect_uri)
        .with_scopes(scopes)
        .with_client_name(
            options
                .client_name
                .clone()
                .unwrap_or_else(|| "Elph MCP Client".to_string()),
        );

    if let Some(client_id) = &options.client_id {
        auth_request = auth_request.with_preregistered_client(client_id.clone());
        if let Some(secret) = &options.client_secret {
            auth_request = auth_request.with_client_secret(secret.clone());
        }
    }
    if let Some(meta_url) = &options.client_metadata_url {
        auth_request = auth_request.with_client_metadata_url(meta_url);
    }

    let session = AuthorizationSession::new(manager, auth_request)
        .await
        .map_err(|(_, e)| {
            // Surface actionable guidance when DCR is blocked (common for gated MCP hosts like Figma).
            let msg = e.to_string();
            if msg.contains("Registration failed") || msg.contains("registration") {
                anyhow::anyhow!(
                    "start OAuth session: {e}. \
                     Tips: set oauthClientId (pre-registered) or oauthClientMetadataUrl (CIMD) in mcp.json; \
                     some hosts (e.g. Figma) only allowlisted client_name values for dynamic registration. \
                     Also ensure scopes include what the server requires (e.g. mcp:connect)."
                )
            } else {
                anyhow::anyhow!("start OAuth session: {e}")
            }
        })?;
    let auth_url = session.get_authorization_url().to_string();

    log::info!("opening browser for MCP OAuth: server={server_name} auth_url={auth_url}");
    println!("Open this URL to authorize MCP server '{server_name}':\n  {auth_url}\n");
    if options.open_browser
        && let Err(error) = open_browser(&auth_url)
    {
        log::warn!("failed to open browser; paste the URL manually: {error}");
    }
    let callback_url = wait_for_oauth_callback(listener)
        .await
        .context("wait for OAuth callback")?;
    let _token = session
        .handle_callback_url(&callback_url)
        .await
        .map_err(|e| anyhow::anyhow!("OAuth token exchange failed: {e}"))?;
    let credentials = session
        .get_credentials()
        .await
        .map_err(|e| anyhow::anyhow!("read OAuth credentials: {e}"))?;
    let client_id = credentials.0;
    // credentials.1 is the OAuthTokenResponse (already persisted by the session).

    println!(
        "Authorized MCP server '{server_name}'. Credentials saved (encrypted) to {}.",
        auth_store_path.display()
    );

    Ok(McpOAuthFlowResult {
        server_name: server_name.to_string(),
        credentials_path: auth_store_path.to_path_buf(),
        client_id,
    })
}

/// Scopes-only convenience wrapper.
pub async fn run_oauth_flow_with_scopes(
    server_name: &str,
    server_url: &str,
    auth_store_path: &Path,
    scopes: &[&str],
) -> Result<McpOAuthFlowResult> {
    run_oauth_flow(
        server_name,
        server_url,
        auth_store_path,
        McpOAuthFlowOptions {
            scopes: scopes.iter().map(|s| (*s).to_string()).collect(),
            open_browser: true,
            ..Default::default()
        },
    )
    .await
}

/// Build an [`AuthorizationManager`] with file-backed credentials for an existing session.
pub async fn authorization_manager_from_store(
    server_url: &str,
    auth_store_path: &Path,
    server_name: &str,
) -> Result<Option<AuthorizationManager>> {
    if !has_stored_credentials(auth_store_path, server_name) {
        return Ok(None);
    }
    let store = FileCredentialStore::new(auth_store_path, server_name);
    let mut manager = AuthorizationManager::new(server_url)
        .await
        .map_err(|e| anyhow::anyhow!("init OAuth manager: {e}"))?;
    manager.set_credential_store(store);
    let ready = manager
        .initialize_from_store()
        .await
        .map_err(|e| anyhow::anyhow!("load OAuth credentials: {e}"))?;
    if ready { Ok(Some(manager)) } else { Ok(None) }
}

/// Resolve a (possibly refreshed) OAuth access token for a server.
pub async fn resolve_oauth_access_token(
    server_url: &str,
    auth_store_path: &Path,
    server_name: &str,
) -> Result<Option<String>> {
    let Some(manager) = authorization_manager_from_store(server_url, auth_store_path, server_name).await? else {
        return Ok(None);
    };
    let token = manager
        .get_access_token()
        .await
        .map_err(|e| anyhow::anyhow!("OAuth access token for \"{server_name}\": {e}"))?;
    Ok(Some(token))
}

fn open_browser(url: &str) -> Result<()> {
    let status = {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open").arg(url).status()
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open").arg(url).status()
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", url])
                .status()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = url;
            return Err(anyhow::anyhow!("opening a browser is not supported on this platform"));
        }
    };
    status.context("launch browser")?;
    Ok(())
}

async fn wait_for_oauth_callback(listener: TcpListener) -> Result<String> {
    let (mut socket, _) = listener.accept().await.context("accept OAuth callback connection")?;
    let mut buf = vec![0u8; 8192];
    let n = socket.read(&mut buf).await.context("read OAuth callback request")?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let path_line = request.lines().next().context("empty OAuth callback request")?;
    let path = path_line
        .split_whitespace()
        .nth(1)
        .context("malformed OAuth callback request line")?;
    let host = listener.local_addr()?;
    let callback_url = format!("http://{}{path}", host);

    let body = b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n\
<!DOCTYPE html><html><body><h1>Authorization complete</h1>\
<p>You can close this window and return to Elph.</p></body></html>";
    let _ = socket.write_all(body).await;
    let _ = socket.shutdown().await;

    if !path.contains("code=") {
        anyhow::bail!("OAuth callback missing authorization code: {path}");
    }
    Ok(callback_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn path_builder_defaults_to_auth_json() {
        let path = AuthStorePathBuilder::new().base_dir("/home/u/.elph").build();
        assert_eq!(path, PathBuf::from("/home/u/.elph/auth.json"));
    }

    #[test]
    fn path_builder_custom_file_and_explicit_path() {
        let path = AuthStorePathBuilder::new()
            .base_dir("/home/u/.acme")
            .file_name("creds.json")
            .build();
        assert_eq!(path, PathBuf::from("/home/u/.acme/creds.json"));

        let path = AuthStorePathBuilder::new()
            .base_dir("/ignored")
            .path("/var/lib/acme/auth.json")
            .build();
        assert_eq!(path, PathBuf::from("/var/lib/acme/auth.json"));
    }

    #[test]
    fn auth_store_path_helper() {
        assert_eq!(auth_store_path(Path::new("/tmp/cfg")), PathBuf::from("/tmp/cfg/auth.json"));
    }

    #[tokio::test]
    async fn multi_server_store_encrypted_roundtrip() {
        let key = Aes256Key::generate();
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");

        let a = FileCredentialStore::with_key(&path, "server-a", key.clone());
        let b = FileCredentialStore::with_key(&path, "server-b", key.clone());

        let creds_a = StoredCredentials::new("client-a".into(), None, vec!["read".into()], Some(1));
        a.save(creds_a.clone()).await.unwrap();

        let creds_b = StoredCredentials::new("client-b".into(), None, vec![], Some(2));
        b.save(creds_b.clone()).await.unwrap();

        let raw = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(
            raw.contains("enc:") && !raw.contains("ciphertext"),
            "expected plain JSON with enc: fields, got: {raw}"
        );
        assert!(!raw.contains("client-a"), "client id must not appear in plaintext: {raw}");
        let mut lock_sidecar = path.as_os_str().to_os_string();
        lock_sidecar.push(".lock");
        assert!(!std::path::PathBuf::from(lock_sidecar).exists());

        let loaded_a = a.load().await.unwrap().unwrap();
        assert_eq!(loaded_a.client_id, "client-a");
        let loaded_b = b.load().await.unwrap().unwrap();
        assert_eq!(loaded_b.client_id, "client-b");

        a.clear().await.unwrap();
        assert!(b.load().await.unwrap().is_some());
        assert!(path.exists());

        b.clear().await.unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn concurrent_saves_do_not_lose_entries() {
        let key = Aes256Key::generate();
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");

        let mut handles = Vec::new();
        for i in 0..12 {
            let path = path.clone();
            let key = key.clone();
            handles.push(tokio::spawn(async move {
                let store = FileCredentialStore::with_key(&path, format!("server-{i}"), key);
                let creds = StoredCredentials::new(format!("client-{i}"), None, vec![], Some(i as u64));
                store.save(creds).await.unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let file = AuthStoreFile::load_from_path_with_key(&path, &key).await.unwrap();
        assert_eq!(file.mcp.len(), 12, "lost entries under concurrent save: {:?}", file.mcp.keys());
        for i in 0..12 {
            let store = FileCredentialStore::with_key(&path, format!("server-{i}"), key.clone());
            let loaded = store.load().await.unwrap().expect("entry present");
            assert_eq!(loaded.client_id, format!("client-{i}"));
        }
    }

    #[test]
    fn store_builder_requires_server_key() {
        let result = FileCredentialStore::builder().base_dir("/tmp").build();
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("server_key"));
    }
}
