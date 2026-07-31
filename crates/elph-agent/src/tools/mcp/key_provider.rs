//! OS keychain master key for the sealed auth store (zero-trust).
//!
//! The AES-256 key never lives on disk next to `auth.json`. It is stored only
//! in the platform keychain (macOS Keychain / Windows Credential Locker /
//! freedesktop Secret Service).
//!
//! Tests may inject a key via [`set_process_master_key_for_tests`].

use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use keyring::Entry;

use super::crypto::Aes256Key;

/// Keychain service id for Elph auth store master keys.
pub const KEYCHAIN_SERVICE: &str = "space.elph.auth";

/// Keychain account for the current auth-store master key generation.
pub const KEYCHAIN_ACCOUNT: &str = "auth-store-master-v2";

const KEY_LEN: usize = 32;

static PROCESS_KEY_OVERRIDE: OnceLock<Mutex<Option<Aes256Key>>> = OnceLock::new();

fn process_override() -> &'static Mutex<Option<Aes256Key>> {
    PROCESS_KEY_OVERRIDE.get_or_init(|| Mutex::new(None))
}

/// Install a process-local master key (unit tests / headless CI). Clears keychain use.
pub fn set_process_master_key_for_tests(key: Aes256Key) {
    *process_override().lock().unwrap_or_else(|e| e.into_inner()) = Some(key);
}

/// Clear process-local override (tests).
pub fn clear_process_master_key_for_tests() {
    *process_override().lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Load the master key from the process override, else OS keychain (create if missing).
pub fn load_or_create_master_key() -> Result<Aes256Key> {
    if let Some(key) = process_override()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    {
        return Ok(key);
    }

    // Optional CI escape hatch: full 32-byte key as URL-safe base64 (no pad).
    if let Ok(b64) = std::env::var("ELPH_AUTH_MASTER_KEY_B64") {
        let trimmed = b64.trim();
        if !trimmed.is_empty() {
            return key_from_b64(trimmed);
        }
    }

    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .context("open OS keychain entry for Elph auth master key")?;

    match entry.get_password() {
        Ok(secret) => key_from_b64(secret.trim()).context("decode master key from keychain"),
        Err(keyring::Error::NoEntry) => {
            let key = Aes256Key::generate();
            let encoded = URL_SAFE_NO_PAD.encode(key.as_bytes());
            entry
                .set_password(&encoded)
                .context("store master key in OS keychain (zero-trust AES-256)")?;
            Ok(key)
        }
        Err(e) => bail!(
            "OS keychain unavailable for auth store master key: {e}. \
             Secrets cannot be stored without a keychain (or ELPH_AUTH_MASTER_KEY_B64 for CI)."
        ),
    }
}

fn key_from_b64(secret: &str) -> Result<Aes256Key> {
    let bytes = URL_SAFE_NO_PAD
        .decode(secret.trim())
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(secret.trim()))
        .context("master key is not valid base64")?;
    if bytes.len() != KEY_LEN {
        bail!("master key must be {KEY_LEN} bytes, got {}", bytes.len());
    }
    let mut arr = [0u8; KEY_LEN];
    arr.copy_from_slice(&bytes);
    Ok(Aes256Key::from_bytes(arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_override_roundtrip() {
        clear_process_master_key_for_tests();
        let key = Aes256Key::generate();
        let bytes = *key.as_bytes();
        set_process_master_key_for_tests(key);
        let loaded = load_or_create_master_key().unwrap();
        assert_eq!(loaded.as_bytes(), &bytes);
        clear_process_master_key_for_tests();
    }
}
