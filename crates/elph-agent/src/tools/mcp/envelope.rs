//! AES-256-GCM envelope seal for the auth store (format v2).
//!
//! On-disk JSON (no secrets in cleartext):
//! ```json
//! { "v": 2, "alg": "aes-256-gcm", "nonce": "…", "ciphertext": "…" }
//! ```
//!
//! Ciphertext is the AES-GCM encryption of the logical [`AuthStoreFile`] JSON.

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use super::crypto::Aes256Key;
use super::crypto::decrypt_sync_bytes;

/// On-disk envelope version.
pub const AUTH_STORE_FORMAT_VERSION: u32 = 2;

const ALG: &str = "aes-256-gcm";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStoreEnvelope {
    pub v: u32,
    pub alg: String,
    /// URL-safe base64 (no pad) of 12-byte nonce.
    pub nonce: String,
    /// URL-safe base64 (no pad) of ciphertext+tag.
    pub ciphertext: String,
}

/// Unseal a v2 envelope into logical store JSON bytes.
pub fn unseal_store(key: &Aes256Key, envelope: &AuthStoreEnvelope) -> Result<Vec<u8>> {
    if envelope.v != AUTH_STORE_FORMAT_VERSION {
        bail!(
            "unsupported auth store format v{} (expected {AUTH_STORE_FORMAT_VERSION}); \
             re-run provider/MCP connect — no legacy migration",
            envelope.v
        );
    }
    if envelope.alg != ALG {
        bail!("unsupported auth store alg {:?}", envelope.alg);
    }
    let nonce = URL_SAFE_NO_PAD
        .decode(envelope.nonce.trim())
        .context("decode auth store nonce")?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(envelope.ciphertext.trim())
        .context("decode auth store ciphertext")?;
    decrypt_sync_bytes(key, &nonce, &ciphertext).context("decrypt auth store envelope")
}

/// True when raw file bytes look like a v2 sealed envelope (not cleartext legacy).
pub fn looks_like_envelope(bytes: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return false;
    };
    v.get("v").and_then(|x| x.as_u64()) == Some(AUTH_STORE_FORMAT_VERSION as u64)
        && v.get("ciphertext").is_some()
        && v.get("nonce").is_some()
}
