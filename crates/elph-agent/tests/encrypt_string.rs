//! Integration tests for AES-256-GCM string encryption (`enc:` format).

#![cfg(feature = "mcp")]

use std::sync::Arc;

use elph_agent::mcp::Aes256Key;
use elph_agent::mcp::ENC_PREFIX;
use elph_agent::mcp::decrypt_json_async;
use elph_agent::mcp::decrypt_string_async;
use elph_agent::mcp::decrypt_string_sync;
use elph_agent::mcp::encrypt_json_async;
use elph_agent::mcp::encrypt_string_async;
use elph_agent::mcp::encrypt_string_sync;
use elph_agent::mcp::is_encrypted_value;
use tempfile::tempdir;

#[tokio::test]
async fn string_encrypt_decrypt_integration() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("t.key");
    let key = Arc::new(Aes256Key::load_or_create(&key_path).await.unwrap());

    let samples = [
        "token-abc-xyz",
        "line1\nline2\ttab",
        "🔐 unicode — café — 中文",
        "",
        &"z".repeat(50_000),
    ];

    for plain in samples {
        let enc = encrypt_string_async(Arc::clone(&key), plain).await.unwrap();
        assert!(enc.starts_with(ENC_PREFIX), "{enc}");
        assert!(is_encrypted_value(&enc));
        if !plain.is_empty() {
            assert!(!enc.contains(plain), "ciphertext leaked plaintext");
        }
        let back = decrypt_string_async(Arc::clone(&key), enc).await.unwrap();
        assert_eq!(back, plain);
    }
}

#[tokio::test]
async fn key_reload_from_disk_roundtrip() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("persist.key");

    let enc = {
        let key = Arc::new(Aes256Key::load_or_create(&key_path).await.unwrap());
        encrypt_string_async(key, "across-reload").await.unwrap()
    };

    let key2 = Arc::new(Aes256Key::load(&key_path).await.unwrap());
    assert_eq!(decrypt_string_async(key2, enc).await.unwrap(), "across-reload");
}

#[tokio::test]
async fn json_blob_roundtrip() {
    let key = Arc::new(Aes256Key::generate());
    let value = serde_json::json!({
        "client_id": "elph",
        "scopes": ["read", "write"],
        "nested": { "ok": true }
    });
    let enc = encrypt_json_async(Arc::clone(&key), value.clone()).await.unwrap();
    let back: serde_json::Value = decrypt_json_async(key, enc).await.unwrap();
    assert_eq!(back, value);
}

#[test]
fn sync_api_matches_async_format() {
    let key = Aes256Key::generate();
    let enc = encrypt_string_sync(&key, "sync-secret").unwrap();
    assert!(enc.starts_with(ENC_PREFIX));
    assert_eq!(decrypt_string_sync(&key, &enc).unwrap(), "sync-secret");
}

#[tokio::test]
async fn wrong_key_or_tamper_fails() {
    let key = Arc::new(Aes256Key::generate());
    let enc = encrypt_string_async(Arc::clone(&key), "data").await.unwrap();

    let other = Arc::new(Aes256Key::generate());
    assert!(decrypt_string_async(other, enc.clone()).await.is_err());

    // Tamper last char of base64 payload
    let mut bad = enc.clone();
    if let Some(c) = bad.pop() {
        bad.push(if c == 'A' { 'B' } else { 'A' });
    }
    assert!(decrypt_string_async(key, bad).await.is_err());
}
