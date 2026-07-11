//! AES-256-GCM string encryption demo (`enc:` prefix).
//!
//! Used by MCP auth store and available as a general helper for at-rest secrets.
//!
//! ```bash
//! # Generate a key file, encrypt a string, decrypt it back
//! cargo run -p elph-agent --features mcp --example encrypt_string -- \
//!   encrypt --key /tmp/elph-demo.key --text "hello secret"
//!
//! cargo run -p elph-agent --features mcp --example encrypt_string -- \
//!   decrypt --key /tmp/elph-demo.key --cipher 'enc:…'
//!
//! # Round-trip demo (prints enc string + plaintext)
//! cargo run -p elph-agent --features mcp --example encrypt_string -- demo
//!
//! # Encrypt JSON object
//! cargo run -p elph-agent --features mcp --example encrypt_string -- \
//!   encrypt-json --key /tmp/elph-demo.key --json '{"token":"abc"}'
//! ```
//!
//! Format: `enc:` + URL-safe base64 (no pad) of `nonce(12) || ciphertext+tag`.
//! Crypto runs on `spawn_blocking` so the async runtime is not blocked.

use std::path::PathBuf;
use std::sync::Arc;

use elph_agent::{
    Aes256Key, ENC_PREFIX, decrypt_json_async, decrypt_string_async, encrypt_json_async, encrypt_string_async,
    is_encrypted_value,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help") {
        print_help();
        return Ok(());
    }

    let cmd = args.remove(0);
    match cmd.as_str() {
        "demo" => cmd_demo().await,
        "encrypt" => {
            let (key_path, text) = parse_key_and_flag(&args, "--text")?;
            let key = Arc::new(Aes256Key::load_or_create(&key_path).await?);
            let enc = encrypt_string_async(key, text).await?;
            println!("{enc}");
            Ok(())
        }
        "decrypt" => {
            let (key_path, cipher) = parse_key_and_flag(&args, "--cipher")?;
            if !is_encrypted_value(&cipher) {
                anyhow::bail!("cipher must start with {ENC_PREFIX}");
            }
            let key = Arc::new(Aes256Key::load(&key_path).await?);
            let plain = decrypt_string_async(key, cipher).await?;
            println!("{plain}");
            Ok(())
        }
        "encrypt-json" => {
            let (key_path, json_str) = parse_key_and_flag(&args, "--json")?;
            let value: serde_json::Value = serde_json::from_str(&json_str)?;
            let key = Arc::new(Aes256Key::load_or_create(&key_path).await?);
            let enc = encrypt_json_async(key, value).await?;
            println!("{enc}");
            Ok(())
        }
        "decrypt-json" => {
            let (key_path, cipher) = parse_key_and_flag(&args, "--cipher")?;
            let key = Arc::new(Aes256Key::load(&key_path).await?);
            let value: serde_json::Value = decrypt_json_async(key, cipher).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        other => anyhow::bail!("unknown command: {other}\n\n{}", help_text()),
    }
}

async fn cmd_demo() -> anyhow::Result<()> {
    println!("── AES-256-GCM string encryption demo ──");
    println!();

    let dir = tempfile::tempdir()?;
    let key_path = dir.path().join("demo.key");
    let key = Arc::new(Aes256Key::load_or_create(&key_path).await?);
    println!("key file : {}", key_path.display());
    println!("key size : {} bytes", key.as_bytes().len());
    println!();

    let samples = [
        "simple-secret",
        "hello 🔐 unicode café",
        "",
        r#"{"access_token":"tok_abc","scope":"read"}"#,
    ];

    for (i, plain) in samples.iter().enumerate() {
        let enc = encrypt_string_async(Arc::clone(&key), *plain).await?;
        let back = decrypt_string_async(Arc::clone(&key), enc.clone()).await?;
        assert_eq!(back, *plain);
        let preview = if enc.len() > 64 {
            format!("{}…", &enc[..64])
        } else {
            enc.clone()
        };
        println!("[{i}] plaintext  {:?}", if plain.is_empty() { "(empty)" } else { plain });
        println!("    ciphertext {preview}");
        println!("    prefix ok  {}", is_encrypted_value(&enc));
        println!("    roundtrip  ok");
        println!();
    }

    // Nonce randomness: same input twice → different ciphertext
    let a = encrypt_string_async(Arc::clone(&key), "same").await?;
    let b = encrypt_string_async(Arc::clone(&key), "same").await?;
    println!("nonce check: two encryptions of \"same\" differ = {}", a != b);
    println!();

    // JSON helper
    let obj = serde_json::json!({"server": "deepwiki", "note": "demo"});
    let enc_json = encrypt_json_async(Arc::clone(&key), obj.clone()).await?;
    let back_json: serde_json::Value = decrypt_json_async(key, enc_json.clone()).await?;
    println!("json encrypt : {}…", &enc_json[..enc_json.len().min(48)]);
    println!("json decrypt : {back_json}");
    assert_eq!(back_json, obj);

    println!();
    println!("Done. Key left at {} (temp dir cleaned on exit).", key_path.display());
    Ok(())
}

fn parse_key_and_flag(args: &[String], value_flag: &str) -> anyhow::Result<(PathBuf, String)> {
    let mut key_path = None;
    let mut value = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--key" => {
                i += 1;
                key_path = Some(PathBuf::from(args.get(i).ok_or_else(|| anyhow::anyhow!("--key needs a path"))?));
            }
            f if f == value_flag => {
                i += 1;
                value = Some(
                    args.get(i)
                        .ok_or_else(|| anyhow::anyhow!("{value_flag} needs a value"))?
                        .clone(),
                );
            }
            other => anyhow::bail!("unknown arg: {other}"),
        }
        i += 1;
    }
    let key_path = key_path.ok_or_else(|| anyhow::anyhow!("missing --key <path>"))?;
    let value = value.ok_or_else(|| anyhow::anyhow!("missing {value_flag} <value>"))?;
    Ok((key_path, value))
}

fn help_text() -> String {
    format!(
        "Usage:\n\
         \n\
           encrypt_string demo\n\
           encrypt_string encrypt --key <path> --text <string>\n\
           encrypt_string decrypt --key <path> --cipher <enc:…>\n\
           encrypt_string encrypt-json --key <path> --json <json>\n\
           encrypt_string decrypt-json --key <path> --cipher <enc:…>\n\
         \n\
         Ciphertext format: {ENC_PREFIX}<url-safe-base64(nonce||ciphertext)>\n\
         Key file: 32 raw bytes (created with mode 0600 on first encrypt).\n"
    )
}

fn print_help() {
    print!("{}", help_text());
}
