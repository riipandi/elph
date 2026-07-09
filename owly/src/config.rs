//! Configuration handling for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/constants.ts` and `src/env.ts`. Original MIT License, Copyright (c) 2026 LangChain.
//!
//! Extended to support all providers from `elph-ai` with automatic detection.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::constants::{provider_config, resolve_configured_provider, resolve_model_id};

/// Owly configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Provider to use (e.g., "opencode", "anthropic", "openai")
    pub provider: String,

    /// Model ID to use (e.g., "big-pickle", "claude-sonnet-5")
    pub model_id: String,

    /// Working directory
    pub cwd: PathBuf,
}

impl Config {
    /// Create config from environment and CLI args
    pub fn resolve(model_override: Option<&str>, cwd: &Path) -> Result<Self> {
        // Check if model_override contains provider/model format
        let (provider_override, model_override) = if let Some(model) = model_override
            && let Some((provider, model_id)) = model.split_once('/')
            && provider_config(provider).is_some()
        {
            (Some(provider.to_string()), Some(model_id.to_string()))
        } else if let Some(model) = model_override {
            (None, Some(model.to_string()))
        } else {
            (None, None)
        };

        let file_cfg = load_config_file();

        // Resolve provider (CLI override > env > config file > auto-detect)
        let provider = provider_override.unwrap_or_else(|| {
            std::env::var(crate::constants::OWLY_PROVIDER_ENV_KEY)
                .ok()
                .filter(|p| provider_config(p).is_some())
                .or_else(|| file_cfg.as_ref().and_then(|f| f.provider.clone()))
                .unwrap_or_else(|| resolve_configured_provider().to_string())
        });

        // Resolve model ID (CLI override > env > config file > provider default)
        let model_id = if let Some(model) = model_override {
            model
        } else if let Ok(model) = std::env::var(crate::constants::OWLY_MODEL_ID_ENV_KEY)
            && !model.trim().is_empty()
        {
            model
        } else if let Some(file) = file_cfg.as_ref().and_then(|f| f.model_id.clone()) {
            file
        } else {
            resolve_model_id(None)
        };

        // Validate provider
        let provider_cfg = provider_config(&provider).with_context(|| format!("Unknown provider: {provider}"))?;

        // Check API key (warn but don't fail - agent will handle it)
        if std::env::var(provider_cfg.api_key_env_key).is_err() {
            eprintln!(
                "Warning: {} environment variable not set for {} provider.",
                provider_cfg.api_key_env_key, provider_cfg.label
            );
        }

        Ok(Config {
            provider,
            model_id,
            cwd: cwd.to_path_buf(),
        })
    }

    /// Get the API key environment variable name for the current provider
    pub fn api_key_env_key(&self) -> &str {
        provider_config(&self.provider)
            .map(|c| c.api_key_env_key)
            .unwrap_or("UNKNOWN_API_KEY")
    }

    /// Get the provider label for display
    pub fn provider_label(&self) -> &str {
        provider_config(&self.provider)
            .map(|c| c.label)
            .unwrap_or(&self.provider)
    }

    /// Check if API key is available
    pub fn has_api_key(&self) -> bool {
        std::env::var(self.api_key_env_key()).is_ok()
    }

    /// Get provider as string for model lookup
    pub fn provider_str(&self) -> &str {
        &self.provider
    }

    /// Get model ID for elph-ai lookup (format: provider/model)
    pub fn elph_model_id(&self) -> String {
        format!("{}/{}", self.provider, self.model_id)
    }
}

/// Load configuration from ~/.owly/config.json if it exists
pub fn load_config_file() -> Option<ConfigFile> {
    let config_path = dirs().join("config.json");
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

/// Save configuration to ~/.owly/config.json
pub fn save_config_file(config: &ConfigFile) -> Result<()> {
    let dir = dirs();
    std::fs::create_dir_all(&dir).context("Failed to create config directory")?;
    let config_path = dir.join("config.json");
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(config_path, content)?;
    Ok(())
}

/// Configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub provider: Option<String>,
    pub model_id: Option<String>,
}

fn dirs() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".owly")
}
