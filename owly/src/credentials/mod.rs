//! Credential management for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/env.ts`. Original MIT License, Copyright (c) 2026 LangChain.

mod auth_context;
mod oauth_callbacks;
mod oauth_store;

pub use auth_context::OwlyAuthContext;
pub use oauth_callbacks::{DialoguerAuthCallbacks, run_oauth_login};
pub use oauth_store::OwlyCredentialStore;

use anyhow::{Context, Result};
use elph_ai::builtin_oauth_provider_ids;
use memchr::memchr;
use std::collections::HashMap;
use std::path::PathBuf;

/// Managed environment keys
pub const MANAGED_ENV_KEYS: &[&str] = &[
    "OPENCODE_API_KEY",
    "OPENROUTER_API_KEY",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_BASE_URL",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_COMPATIBLE_API_KEY",
    "OPENAI_COMPATIBLE_BASE_URL",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "DEEPSEEK_API_KEY",
    "GROQ_API_KEY",
    "FIREWORKS_API_KEY",
    "TOGETHER_API_KEY",
    "MISTRAL_API_KEY",
    "NVIDIA_API_KEY",
    "OWLY_PROVIDER",
    "OWLY_MODEL_ID",
];

/// Load environment from ~/.owly/.env and seed process env when unset.
pub fn load_env() -> Result<HashMap<String, String>> {
    let env = read_env_file()?;

    // Apply to process env when not already set
    for (key, value) in &env {
        if std::env::var(key).is_err() {
            // SAFETY: Setting env before Owly spawns concurrent work that reads keys.
            unsafe {
                std::env::set_var(key, value);
            }
        }
    }

    Ok(env)
}

/// Save environment updates to ~/.owly/.env
pub fn save_env(updates: &HashMap<String, String>) -> Result<()> {
    let mut current = load_env().unwrap_or_default();

    // Merge updates
    for (key, value) in updates {
        current.insert(key.clone(), value.clone());
    }

    let dir = env_dir_internal();
    std::fs::create_dir_all(&dir).context("Failed to create env directory")?;

    // Write env file
    let lines: Vec<String> = current
        .iter()
        .map(|(k, v)| format!("{}={}", k, format_env_value(v)))
        .collect();

    std::fs::write(env_path(), lines.join("\n") + "\n")?;
    secure_env_dir()?;

    // Apply to process env
    for (key, value) in updates {
        // SAFETY: We are setting environment variables before any threads are spawned
        unsafe {
            std::env::set_var(key, value);
        }
    }

    Ok(())
}

/// Check if any API key is configured
pub fn has_any_api_key() -> bool {
    has_provider_api_key_for_any()
}

/// Check if any known provider API key is set and non-empty.
pub fn has_provider_api_key_for_any() -> bool {
    MANAGED_ENV_KEYS
        .iter()
        .filter(|k| k.ends_with("_API_KEY"))
        .any(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()).is_some())
}

/// Returns true when OAuth credentials exist for any builtin OAuth provider.
pub fn has_any_stored_oauth() -> bool {
    builtin_oauth_provider_ids().into_iter().any(has_stored_oauth)
}

/// Returns true when `~/.owly/oauth-credentials.json` contains OAuth for `provider_id`.
pub fn has_stored_oauth(provider_id: &str) -> bool {
    let path = oauth_credentials_path();
    if !path.exists() {
        return false;
    }
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(_) => return false,
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return false,
    };
    parsed.get("credentials").and_then(|c| c.get(provider_id)).is_some()
}

pub fn oauth_credentials_path() -> PathBuf {
    env_dir_internal().join("oauth-credentials.json")
}

pub fn env_dir() -> PathBuf {
    env_dir_internal()
}

pub fn env_path() -> PathBuf {
    env_path_internal()
}

pub(crate) fn env_dir_internal() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".owly")
}

fn env_path_internal() -> PathBuf {
    env_dir_internal().join(".env")
}

#[cfg(unix)]
pub fn secure_env_dir() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = env_dir_internal();
    if dir.exists() {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn secure_env_dir() -> Result<()> {
    Ok(())
}

/// Parse environment value, handling quoted strings.
///
/// Escapes match OpenWiki `src/env.ts`: `\\`, `\"`, `\n`, and `\r` (Windows multi-line values).
pub fn parse_env_value(value: &str) -> String {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value[1..value.len() - 1]
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        value.to_string()
    }
}

/// Format environment value for storage (always double-quoted with escapes).
pub fn format_env_value(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

/// One managed credential key for diagnostics (display only; secrets masked).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialDiagnostic {
    pub key: String,
    pub set: bool,
    /// Masked preview when set (never the full secret for `*_API_KEY` keys).
    pub display: String,
}

/// Report which managed keys are set (process env wins over `~/.owly/.env` file).
///
/// Does not mutate process environment (reads the file only).
pub fn get_credential_diagnostics() -> Result<Vec<CredentialDiagnostic>> {
    let file_env = read_env_file().unwrap_or_default();
    let mut out = Vec::with_capacity(MANAGED_ENV_KEYS.len());
    for &key in MANAGED_ENV_KEYS {
        let from_process = std::env::var(key).ok().filter(|v| !v.is_empty());
        let from_file = file_env.get(key).cloned().filter(|v| !v.is_empty());
        let value = from_process.or(from_file);
        let set = value.is_some();
        let display = match value {
            Some(v) if key.ends_with("_API_KEY") || key.contains("TOKEN") || key.contains("SECRET") => mask_secret(&v),
            Some(v) => v,
            None => "(unset)".to_string(),
        };
        out.push(CredentialDiagnostic {
            key: key.to_string(),
            set,
            display,
        });
    }
    Ok(out)
}

/// Read `~/.owly/.env` without applying keys to the process environment.
pub fn read_env_file() -> Result<HashMap<String, String>> {
    let env_path = env_path_internal();
    if !env_path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(&env_path).context("Failed to read environment file")?;
    Ok(parse_env_file_content(&content))
}

fn parse_env_file_content(content: &str) -> HashMap<String, String> {
    let mut env = HashMap::new();
    let mut start = 0usize;
    while start <= content.len() {
        let remaining = &content[start..];
        if remaining.is_empty() {
            break;
        }
        let (line, next_start) = match memchr(b'\n', remaining.as_bytes()) {
            Some(end) => (&remaining[..end], start + end + 1),
            None => (remaining, content.len() + 1),
        };
        let line = line.trim();
        if !line.is_empty()
            && !line.starts_with('#')
            && let Some((key, value)) = line.split_once('=')
        {
            env.insert(key.trim().to_string(), parse_env_value(value.trim()));
        }
        if next_start > content.len() {
            break;
        }
        start = next_start;
    }
    env
}

fn mask_secret(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        return "[REDACTED]".to_string();
    }
    let prefix: String = chars.iter().take(4).collect();
    let suffix: String = chars
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}…{suffix}")
}
