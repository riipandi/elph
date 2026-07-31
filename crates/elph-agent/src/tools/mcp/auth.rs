//! OAuth 2.1 credential storage and authorization helpers for remote MCP servers.
//!
//! Credentials live in a **sealed** AES-256-GCM envelope file (default name
//! [`DEFAULT_AUTH_FILE_NAME`] = `auth.json`). The master key is kept only in the
//! OS keychain (zero-trust) — never as `auth.key` beside the store.
//!
//! Logical payload (after decrypt):
//! ```json
//! { "mcp": { "<server>": { …oauth tokens… } }, "providers": { "<id>": "sk-…" | "env:VAR" } }
//! ```
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
use super::envelope::{looks_like_envelope, seal_store, unseal_store};
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
/// On disk this is sealed as an AES-256-GCM envelope; see [`seal_store`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStoreFile {
    /// Map of MCP server name → OAuth credential JSON object (or null).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp: BTreeMap<String, Value>,
    /// Map of provider ID → API key string or `env:VAR` reference.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<String, Value>,
}

impl AuthStoreFile {
    /// Load and unseal the auth store (OS keychain master key). Missing / empty → empty store.
    ///
    /// Only format v2 envelopes are accepted (no legacy migration).
    pub async fn load_from_path(path: &Path) -> Result<Self, AuthError> {
        let key = load_or_create_master_key()
            .map_err(|e| AuthError::InternalError(format!("auth master key: {e}")))?;
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
        Self::from_sealed_bytes(&bytes, key)
    }

    /// Sync load (CLI probes) using the OS keychain master key.
    pub fn load_from_path_sync(path: &Path) -> Result<Self, AuthError> {
        let key = load_or_create_master_key()
            .map_err(|e| AuthError::InternalError(format!("auth master key: {e}")))?;
        Self::load_from_path_sync_with_key(path, &key)
    }

    /// Sync load with an explicit master key.
    pub fn load_from_path_sync_with_key(path: &Path, key: &Aes256Key) -> Result<Self, AuthError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes =
            std::fs::read(path).map_err(|e| AuthError::InternalError(format!("read auth store: {e}")))?;
        Self::from_sealed_bytes(&bytes, key)
    }

    fn from_sealed_bytes(bytes: &[u8], key: &Aes256Key) -> Result<Self, AuthError> {
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        if !looks_like_envelope(bytes) {
            return Err(AuthError::InternalError(
                "auth store is not a sealed v2 envelope (legacy formats are not supported — re-authenticate)"
                    .into(),
            ));
        }
        let envelope: super::envelope::AuthStoreEnvelope = serde_json::from_slice(bytes)
            .map_err(|e| AuthError::InternalError(format!("parse auth envelope: {e}")))?;
        let plain = unseal_store(key, &envelope)
            .map_err(|e| AuthError::InternalError(format!("unseal auth store: {e}")))?;
        serde_json::from_slice(&plain).map_err(|e| AuthError::InternalError(format!("parse auth payload: {e}")))
    }

    /// Seal and write without taking the store lock (caller must hold [`lock_auth_store`]).
    pub async fn save_to_path_unlocked(&self, path: &Path) -> Result<(), AuthError> {
        let key = load_or_create_master_key()
            .map_err(|e| AuthError::InternalError(format!("auth master key: {e}")))?;
        self.save_to_path_unlocked_with_key(path, &key).await
    }

    /// Seal and write with an explicit master key.
    pub async fn save_to_path_unlocked_with_key(&self, path: &Path, key: &Aes256Key) -> Result<(), AuthError> {
        let plain = serde_json::to_vec(self)
            .map_err(|e| AuthError::InternalError(format!("serialize auth payload: {e}")))?;
        let envelope = seal_store(key, &plain)
            .map_err(|e| AuthError::InternalError(format!("seal auth store: {e}")))?;
        let bytes = serde_json::to_vec_pretty(&envelope)
            .map_err(|e| AuthError::InternalError(format!("serialize auth envelope: {e}")))?;
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
    /// Secrets are only protected by the sealed envelope on disk.
    pub fn set_provider_credential(&mut self, provider_id: &str, credential: String) {
        self.providers
            .insert(provider_id.to_string(), Value::String(credential));
    }

    /// Get provider credential string (API key or `env:VAR`).
    pub fn get_provider_credential(&self, provider_id: &str) -> Option<&str> {
        self.providers.get(provider_id).and_then(|v| v.as_str())
    }

    /// Remove a provider credential. Returns `true` if it existed.
    pub fn remove_provider_credential(&mut self, provider_id: &str) -> bool {
        self.providers.remove(provider_id).is_some()
    }

    /// List all provider IDs that have stored credentials.
    pub fn provider_ids(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// Check if a provider entry is an env-var reference (`env:VAR_NAME`).
    pub fn is_env_ref(&self, provider_id: &str) -> bool {
        self.providers
            .get(provider_id)
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.starts_with(ENV_REF_PREFIX))
    }

    /// Extract the env var name from an `env:…` entry, e.g. `"env:OPENAI_API_KEY"` → `"OPENAI_API_KEY"`.
    pub fn env_var_name(&self, provider_id: &str) -> Option<String> {
        self.providers
            .get(provider_id)
            .and_then(|v| v.as_str())
            .filter(|s| s.starts_with(ENV_REF_PREFIX))
            .map(|s| s[ENV_REF_PREFIX.len()..].to_string())
    }
}

// ---------------------------------------------------------------------------
// Per-server CredentialStore backed by shared encrypted auth.json
// ---------------------------------------------------------------------------

/// File-backed [`CredentialStore`] for **one** MCP server key inside a sealed `auth.json`.
///
/// The whole file is envelope-encrypted; MCP credentials are JSON objects inside the payload.
#[derive(Clone)]
pub struct FileCredentialStore {
    path: PathBuf,
    server_key: String,
    /// When set, used instead of the OS keychain (tests / injectors).
    master_key: Option<Arc<Aes256Key>>,
    cache: Arc<RwLock<Option<StoredCredentials>>>,
}

impl FileCredentialStore {
    /// Create a store for `server_key` inside the shared sealed file at `path`.
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
        if file.mcp.is_empty() && file.providers.is_empty() {
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

/// True when sealed `auth.json` contains an entry for `server_name`.
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
    async fn sealed_provider_roundtrip_no_lock_sidecar() {
        let key = Aes256Key::generate();
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");

        let mut file = AuthStoreFile::default();
        file.set_provider_credential("opencode", "sk-test-secret".into());
        file.save_to_path_with_key(&path, &key).await.unwrap();

        let mut lock_sidecar = path.as_os_str().to_os_string();
        lock_sidecar.push(".lock");
        assert!(!std::path::PathBuf::from(lock_sidecar).exists());
        assert!(!path.with_extension("key").exists());

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"v\": 2") || raw.contains("\"v\":2"));
        assert!(!raw.contains("sk-test-secret"));

        let loaded = AuthStoreFile::load_from_path_with_key(&path, &key).await.unwrap();
        assert_eq!(
            loaded.get_provider_credential("opencode"),
            Some("sk-test-secret")
        );
    }

    #[tokio::test]
    async fn rejects_cleartext_legacy_store() {
        let key = Aes256Key::generate();
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(&path, r#"{"providers":{"x":"sk"}}"#).unwrap();
        let err = AuthStoreFile::load_from_path_with_key(&path, &key).await.unwrap_err();
        assert!(err.to_string().to_ascii_lowercase().contains("envelope") || err.to_string().contains("legacy"));
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

    // Resolve metadata (replaces discover_metadata).
    let _metadata_resolution = manager
        .resolve_metadata()
        .await
        .map_err(|e| anyhow::anyhow!("resolve OAuth metadata: {e}"))?;

    let bind_addr = match options.redirect_port {
        Some(port) => format!("127.0.0.1:{port}"),
        None => "127.0.0.1:0".to_string(),
    };
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind OAuth callback listener on {bind_addr}"))?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let mut auth_request = AuthorizationRequest::new(&redirect_uri)
        .with_scopes(options.scopes.clone())
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
        .map_err(|(_, e)| anyhow::anyhow!("start OAuth session: {e}"))?;
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
        assert!(raw.contains("\"v\":") && raw.contains("ciphertext"), "expected v2 envelope: {raw}");
        assert!(!raw.contains("client-a"), "client id must not appear in plaintext");
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
