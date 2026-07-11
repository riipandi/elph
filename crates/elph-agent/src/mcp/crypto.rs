//! AES-256-GCM helpers for at-rest secrets.
//!
//! Ciphertext is stored as a UTF-8 string with the [`ENC_PREFIX`] (`enc:`) followed by
//! URL-safe base64 (no pad) of `nonce || ciphertext+tag`.
//!
//! Heavy crypto work runs on the blocking thread pool via [`tokio::task::spawn_blocking`]
//! so the async runtime is never blocked.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Prefix for encrypted string values written to disk.
pub const ENC_PREFIX: &str = "enc:";

/// Default filename for the 32-byte AES key next to the auth store.
///
/// For `…/auth.json` the default key path is `…/auth.key`.
pub const DEFAULT_AUTH_KEY_FILE_NAME: &str = "auth.key";

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// 256-bit AES key. Drop clears bytes in-process when possible.
#[derive(Clone)]
pub struct Aes256Key {
    bytes: [u8; KEY_LEN],
}

impl Drop for Aes256Key {
    fn drop(&mut self) {
        for b in &mut self.bytes {
            *b = 0;
        }
    }
}

impl std::fmt::Debug for Aes256Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Aes256Key([REDACTED])")
    }
}

impl Aes256Key {
    /// Create from raw 32 bytes.
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self { bytes }
    }

    /// Generate a new random key.
    pub fn generate() -> Self {
        let key = Aes256Gcm::generate_key(&mut OsRng);
        let mut bytes = [0u8; KEY_LEN];
        bytes.copy_from_slice(key.as_slice());
        Self { bytes }
    }

    /// Load a key from `path`, or create + persist a new one (mode `0600` on Unix).
    ///
    /// I/O runs on the blocking pool.
    pub async fn load_or_create(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        tokio::task::spawn_blocking(move || load_or_create_key_sync(&path))
            .await
            .context("join load_or_create key")?
    }

    /// Load an existing key file (fails if missing).
    pub async fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        tokio::task::spawn_blocking(move || load_key_sync(&path))
            .await
            .context("join load key")?
    }

    /// Persist this key to `path` (async, non-blocking for the runtime).
    pub async fn save(&self, path: impl Into<PathBuf>) -> Result<()> {
        let path = path.into();
        let bytes = self.bytes;
        tokio::task::spawn_blocking(move || write_private_bytes_sync(&path, &bytes))
            .await
            .context("join save key")?
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.bytes
    }
}

/// Path of the AES key file associated with an auth store file.
///
/// `auth.json` → `auth.key` (same directory, extension replaced).
pub fn default_auth_key_path(auth_store_path: &Path) -> PathBuf {
    auth_store_path.with_extension("key")
}

/// True when `value` is an encrypted string (`enc:…`).
pub fn is_encrypted_value(value: &str) -> bool {
    value.starts_with(ENC_PREFIX)
}

/// Encrypt plaintext → `enc:<base64url(nonce||ciphertext)>` on a blocking thread.
pub async fn encrypt_async(key: Arc<Aes256Key>, plaintext: Vec<u8>) -> Result<String> {
    tokio::task::spawn_blocking(move || encrypt_sync(&key, &plaintext))
        .await
        .context("join encrypt")?
}

/// Decrypt an `enc:…` string on a blocking thread.
pub async fn decrypt_async(key: Arc<Aes256Key>, encoded: String) -> Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || decrypt_sync(&key, &encoded))
        .await
        .context("join decrypt")?
}

/// Encrypt a UTF-8 string → `enc:…` ciphertext string.
pub async fn encrypt_string_async(key: Arc<Aes256Key>, plaintext: impl Into<String>) -> Result<String> {
    let plaintext = plaintext.into();
    encrypt_async(key, plaintext.into_bytes()).await
}

/// Decrypt an `enc:…` string back to UTF-8 plaintext.
pub async fn decrypt_string_async(key: Arc<Aes256Key>, encoded: impl Into<String>) -> Result<String> {
    let bytes = decrypt_async(key, encoded.into()).await?;
    String::from_utf8(bytes).context("decrypted payload is not valid UTF-8")
}

/// Encrypt JSON-serializable value to an `enc:` string.
pub async fn encrypt_json_async<T: serde::Serialize + Send + 'static>(key: Arc<Aes256Key>, value: T) -> Result<String> {
    let plaintext = serde_json::to_vec(&value).context("serialize for encrypt")?;
    encrypt_async(key, plaintext).await
}

/// Decrypt an `enc:` string into a JSON value.
pub async fn decrypt_json_async<T: serde::de::DeserializeOwned + Send + 'static>(
    key: Arc<Aes256Key>,
    encoded: String,
) -> Result<T> {
    let plain = decrypt_async(key, encoded).await?;
    serde_json::from_slice(&plain).context("deserialize decrypted payload")
}

/// Synchronous encrypt (for tests / non-async call sites). Prefer [`encrypt_string_async`].
pub fn encrypt_string_sync(key: &Aes256Key, plaintext: &str) -> Result<String> {
    encrypt_sync(key, plaintext.as_bytes())
}

/// Synchronous decrypt (for tests / non-async call sites). Prefer [`decrypt_string_async`].
pub fn decrypt_string_sync(key: &Aes256Key, encoded: &str) -> Result<String> {
    let bytes = decrypt_sync(key, encoded)?;
    String::from_utf8(bytes).context("decrypted payload is not valid UTF-8")
}

fn encrypt_sync(key: &Aes256Key, plaintext: &[u8]) -> Result<String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.bytes));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("AES-256-GCM encrypt failed: {e}"))?;

    let mut packed = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    packed.extend_from_slice(nonce.as_slice());
    packed.extend_from_slice(&ciphertext);

    Ok(format!("{ENC_PREFIX}{}", URL_SAFE_NO_PAD.encode(packed)))
}

fn decrypt_sync(key: &Aes256Key, encoded: &str) -> Result<Vec<u8>> {
    let rest = encoded
        .strip_prefix(ENC_PREFIX)
        .with_context(|| format!("missing {ENC_PREFIX} prefix on encrypted value"))?;
    let packed = URL_SAFE_NO_PAD
        .decode(rest.trim())
        .context("base64 decode encrypted value")?;
    if packed.len() <= NONCE_LEN {
        bail!("encrypted payload too short");
    }
    let (nonce_bytes, ct) = packed.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.bytes));
    cipher
        .decrypt(nonce, ct)
        .map_err(|e| anyhow::anyhow!("AES-256-GCM decrypt failed: {e}"))
}

fn load_or_create_key_sync(path: &Path) -> Result<Aes256Key> {
    if path.exists() {
        return load_key_sync(path);
    }
    let key = Aes256Key::generate();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    write_private_bytes_sync(path, &key.bytes)?;
    Ok(key)
}

fn load_key_sync(path: &Path) -> Result<Aes256Key> {
    let bytes = std::fs::read(path).with_context(|| format!("read key {}", path.display()))?;
    if bytes.len() != KEY_LEN {
        bail!("auth key at {} must be {KEY_LEN} bytes, got {}", path.display(), bytes.len());
    }
    let mut arr = [0u8; KEY_LEN];
    arr.copy_from_slice(&bytes);
    Ok(Aes256Key::from_bytes(arr))
}

fn write_private_bytes_sync(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms).with_context(|| format!("chmod {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn encrypt_decrypt_roundtrip_async() {
        let key = Arc::new(Aes256Key::generate());
        let plain = b"secret payload \x00\xff".to_vec();
        let enc = encrypt_async(Arc::clone(&key), plain.clone()).await.unwrap();
        assert!(enc.starts_with(ENC_PREFIX));
        let out = decrypt_async(key, enc).await.unwrap();
        assert_eq!(out, plain);
    }

    #[tokio::test]
    async fn encrypt_string_roundtrip_unicode() {
        let key = Arc::new(Aes256Key::generate());
        let plain = "hello 🔐 — café — 日本語";
        let enc = encrypt_string_async(Arc::clone(&key), plain).await.unwrap();
        assert!(is_encrypted_value(&enc));
        assert!(enc.starts_with(ENC_PREFIX));
        // Ciphertext must not contain plaintext.
        assert!(!enc.contains("hello"));
        assert!(!enc.contains("café"));
        let out = decrypt_string_async(key, enc).await.unwrap();
        assert_eq!(out, plain);
    }

    #[tokio::test]
    async fn encrypt_string_empty_and_long() {
        let key = Arc::new(Aes256Key::generate());
        let empty = encrypt_string_async(Arc::clone(&key), "").await.unwrap();
        assert_eq!(decrypt_string_async(Arc::clone(&key), empty).await.unwrap(), "");

        let long = "あ".repeat(10_000);
        let enc = encrypt_string_async(Arc::clone(&key), long.clone()).await.unwrap();
        assert_eq!(decrypt_string_async(key, enc).await.unwrap(), long);
    }

    #[tokio::test]
    async fn encrypt_string_is_nondeterministic() {
        // Same plaintext + key must produce different ciphertexts (random nonce).
        let key = Arc::new(Aes256Key::generate());
        let plain = "same secret";
        let a = encrypt_string_async(Arc::clone(&key), plain).await.unwrap();
        let b = encrypt_string_async(Arc::clone(&key), plain).await.unwrap();
        assert_ne!(a, b);
        assert_eq!(decrypt_string_async(Arc::clone(&key), a).await.unwrap(), plain);
        assert_eq!(decrypt_string_async(key, b).await.unwrap(), plain);
    }

    #[tokio::test]
    async fn wrong_key_fails_decrypt() {
        let k1 = Arc::new(Aes256Key::generate());
        let k2 = Arc::new(Aes256Key::generate());
        let enc = encrypt_string_async(k1, "top secret").await.unwrap();
        let err = decrypt_string_async(k2, enc).await.unwrap_err();
        assert!(
            err.to_string().to_ascii_lowercase().contains("decrypt")
                || err.to_string().to_ascii_lowercase().contains("aes"),
            "{err}"
        );
    }

    #[test]
    fn sync_string_helpers_roundtrip() {
        let key = Aes256Key::generate();
        let enc = encrypt_string_sync(&key, "sync-path").unwrap();
        assert!(is_encrypted_value(&enc));
        assert_eq!(decrypt_string_sync(&key, &enc).unwrap(), "sync-path");
    }

    #[test]
    fn decrypt_rejects_missing_prefix_and_garbage() {
        let key = Aes256Key::generate();
        assert!(decrypt_string_sync(&key, "not-encrypted").is_err());
        assert!(decrypt_string_sync(&key, "enc:!!!not-base64!!!").is_err());
        assert!(decrypt_string_sync(&key, "enc:").is_err());
        assert!(decrypt_string_sync(&key, "enc:YQ").is_err()); // too short after decode
    }

    #[test]
    fn is_encrypted_value_prefix() {
        assert!(is_encrypted_value("enc:abc"));
        assert!(!is_encrypted_value("enc"));
        assert!(!is_encrypted_value("ENC:abc"));
        assert!(!is_encrypted_value("plain"));
    }

    #[tokio::test]
    async fn load_or_create_persists_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.key");
        let k1 = Aes256Key::load_or_create(&path).await.unwrap();
        let k2 = Aes256Key::load(&path).await.unwrap();
        assert_eq!(k1.as_bytes(), k2.as_bytes());
        assert!(path.exists());
    }

    #[tokio::test]
    async fn key_file_enables_string_roundtrip_across_process_simulation() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("secrets.key");
        let key = Aes256Key::load_or_create(&key_path).await.unwrap();
        let enc = encrypt_string_async(Arc::new(key), "persisted-secret").await.unwrap();

        // Simulate new process: reload key from disk, decrypt.
        let reloaded = Aes256Key::load(&key_path).await.unwrap();
        let plain = decrypt_string_async(Arc::new(reloaded), enc).await.unwrap();
        assert_eq!(plain, "persisted-secret");
    }

    #[tokio::test]
    async fn json_roundtrip() {
        let key = Arc::new(Aes256Key::generate());
        let value = serde_json::json!({"clientId": "abc", "n": 1});
        let enc = encrypt_json_async(Arc::clone(&key), value.clone()).await.unwrap();
        assert!(is_encrypted_value(&enc));
        let back: serde_json::Value = decrypt_json_async(key, enc).await.unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn default_key_path_from_auth_json() {
        let p = PathBuf::from("/home/u/.elph/auth.json");
        assert_eq!(default_auth_key_path(&p), PathBuf::from("/home/u/.elph/auth.key"));
    }

    #[test]
    fn debug_redacts_key_bytes() {
        let key = Aes256Key::generate();
        let s = format!("{key:?}");
        assert_eq!(s, "Aes256Key([REDACTED])");
        assert!(!s.contains(&format!("{:?}", key.as_bytes())));
    }
}
