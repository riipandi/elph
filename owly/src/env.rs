//! Environment handling for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/env.ts`. Original MIT License, Copyright (c) 2026 LangChain.

use anyhow::Result;

use crate::config::Config;
use crate::constants::{
    provider_config, provider_is_configured, provider_oauth_capable, provider_oauth_only, provider_requires_base_url,
    resolve_configured_provider, resolve_provider_base_url,
};

/// Load and validate environment for running Owly
pub fn setup_environment(config: &Config) -> Result<()> {
    // Load environment from ~/.owly/.env
    crate::credentials::load_env()?;

    if !provider_is_configured(&config.provider) {
        let label = provider_config(&config.provider)
            .map(|c| c.label)
            .unwrap_or_else(|| "Unknown".to_string());
        if provider_oauth_only(&config.provider) {
            anyhow::bail!(
                "OAuth sign-in is required to run Owly with {label}. Run Owly in an interactive terminal to complete setup."
            );
        }
        if provider_oauth_capable(&config.provider) {
            anyhow::bail!(
                "An API key or OAuth sign-in is required to run Owly with {label}. Run Owly in an interactive terminal to complete setup."
            );
        }
        let api_key_env = config.api_key_env_key();
        anyhow::bail!("{api_key_env} is required to run Owly with {label}. Set it in your environment or ~/.owly/.env");
    }

    if provider_requires_base_url(&config.provider) && resolve_provider_base_url(&config.provider).is_none() {
        let label = provider_config(&config.provider)
            .map(|c| c.label)
            .unwrap_or_else(|| "Unknown".to_string());
        let base_key = provider_config(&config.provider)
            .and_then(|c| c.base_url_env_key)
            .unwrap_or("BASE_URL");
        anyhow::bail!("{base_key} is required to run Owly with {label}.");
    }

    Ok(())
}

/// Whether debug logging is enabled (`OWLY_DEBUG=1`).
pub fn is_debug_enabled() -> bool {
    matches!(
        std::env::var("OWLY_DEBUG").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

/// Emit a debug line when `OWLY_DEBUG` is enabled.
pub fn debug_log(message: impl AsRef<str>) {
    if is_debug_enabled() {
        eprintln!("\x1b[2m[debug]\x1b[0m {}", message.as_ref());
    }
}

/// Get environment debug info (redacted)
pub fn get_debug_env() -> Vec<(String, String)> {
    let keys = vec![
        "OWLY_PROVIDER",
        "OWLY_MODEL_ID",
        "OPENCODE_API_KEY",
        "OPENROUTER_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "DEEPSEEK_API_KEY",
    ];

    keys.into_iter()
        .map(|key| {
            let value = std::env::var(key).ok();
            let display = match value {
                None => "unset".to_string(),
                Some(v) if key.contains("KEY") => format!("set(length={})", v.len()),
                Some(v) => format!("set(value={:?})", v),
            };
            (key.to_string(), display)
        })
        .collect()
}

/// Resolve provider from environment with Owly defaults.
pub fn resolve_provider() -> String {
    resolve_configured_provider().to_string()
}
