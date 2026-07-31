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
use super::crypto::{decrypt_sync_bytes, encrypt_sync_bytes};

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

/// Seal logical store JSON bytes into a v2 envelope document.
pub fn seal_store(key: &Aes256Key, plaintext_json: &[u8]) -> Result<AuthStoreEnvelope> {
    let (nonce, ciphertext) = encrypt_sync_bytes(key, plaintext_json)?;
    Ok(AuthStoreEnvelope {
        v: AUTH_STORE_FORMAT_VERSION,
        alg: ALG.to_string(),
        nonce: URL_SAFE_NO_PAD.encode(nonce),
        ciphertext: URL_SAFE_NO_PAD.encode(ciphertext),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::crypto::Aes256Key;

    #[test]
    fn seal_unseal_roundtrip() {
        let key = Aes256Key::generate();
        let plain = br#"{"mcp":{},"providers":{"opencode":"sk-test"}}"#;
        let env = seal_store(&key, plain).unwrap();
        assert_eq!(env.v, 2);
        assert_eq!(env.alg, ALG);
        let out = unseal_store(&key, &env).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn wrong_key_fails() {
        let k1 = Aes256Key::generate();
        let k2 = Aes256Key::generate();
        let env = seal_store(&k1, b"{}").unwrap();
        assert!(unseal_store(&k2, &env).is_err());
    }
}
