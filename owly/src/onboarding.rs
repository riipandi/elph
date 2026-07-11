//! First-run credential onboarding wizard.

use anyhow::{Context, Result};
use dialoguer::{Input, Select};
use std::collections::HashMap;

use crate::config::Config;
use crate::constants::{
    ONBOARDING_PROVIDERS, OWLY_MODEL_ID_ENV_KEY, OWLY_PROVIDER_ENV_KEY, is_valid_model_id, normalize_model_id,
    provider_config, provider_requires_base_url, resolve_provider_base_url,
};
use crate::credentials::{self, save_env};
use crate::startup::provider_has_api_key;

/// Credentials collected during interactive setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupCredentials {
    pub provider: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model_id: String,
}

/// Returns true when interactive setup should run before the first agent call.
pub fn needs_setup(config: &Config) -> bool {
    !provider_has_api_key(&config.provider)
        || (provider_requires_base_url(&config.provider) && resolve_provider_base_url(&config.provider).is_none())
}

/// Validate, persist, and return updated config fields after setup.
pub fn apply_setup(credentials: SetupCredentials, config: &Config) -> Result<Config> {
    let provider = credentials.provider.trim();
    let provider_cfg = provider_config(provider).with_context(|| format!("Unknown provider: {provider}"))?;

    let api_key = credentials.api_key.trim();
    if api_key.is_empty() {
        anyhow::bail!("{} API key is required.", provider_cfg.label);
    }

    let mut updates = HashMap::from([
        (OWLY_PROVIDER_ENV_KEY.to_string(), provider.to_string()),
        (provider_cfg.api_key_env_key.to_string(), api_key.to_string()),
    ]);

    if let Some(base_url_key) = provider_cfg.base_url_env_key {
        let base_url = credentials
            .base_url
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if provider_cfg.requires_base_url && base_url.is_empty() {
            anyhow::bail!("{base_url_key} is required for {}.", provider_cfg.label);
        }
        if !base_url.is_empty() {
            updates.insert(base_url_key.to_string(), base_url);
        }
    }

    let model_id = normalize_model_id(&credentials.model_id);
    if !is_valid_model_id(&model_id) {
        anyhow::bail!("Invalid model ID: {model_id}");
    }
    updates.insert(OWLY_MODEL_ID_ENV_KEY.to_string(), model_id.clone());

    save_env(&updates)?;
    credentials::secure_env_dir()?;

    let mut next = config.clone();
    next.provider = provider.to_string();
    next.model_id = model_id;
    Ok(next)
}

/// Build provider labels for the setup wizard.
pub fn provider_select_items() -> Vec<(String, String)> {
    ONBOARDING_PROVIDERS
        .iter()
        .filter_map(|id| provider_config(id).map(|cfg| (id.to_string(), format!("{} ({id})", cfg.label))))
        .collect()
}

/// Default model for a provider id.
pub fn default_model_for_provider(provider: &str) -> Option<String> {
    provider_config(provider).map(|cfg| cfg.default_model.to_string())
}

/// Whether the setup flow should collect a base URL for this provider.
pub fn setup_collects_base_url(provider: &str) -> bool {
    provider_config(provider).and_then(|cfg| cfg.base_url_env_key).is_some()
}

/// Whether the base URL field is required for this provider.
pub fn setup_base_url_required(provider: &str) -> bool {
    provider_config(provider)
        .map(|cfg| cfg.requires_base_url)
        .unwrap_or(false)
}

/// API key env label for prompts.
pub fn api_key_label(provider: &str) -> Option<String> {
    provider_config(provider).map(|cfg| format!("{} API key", cfg.label))
}

/// Base URL env key label for prompts.
pub fn base_url_label(provider: &str) -> Option<String> {
    provider_config(provider).and_then(|cfg| {
        cfg.base_url_env_key.map(|key| {
            if cfg.requires_base_url {
                format!("{key} (required)")
            } else {
                format!("{key} (optional, Enter to skip)")
            }
        })
    })
}

/// Run the interactive credential wizard and persist settings to `~/.owly/.env`.
pub fn run_wizard(config: &mut Config) -> Result<()> {
    println!();
    println!("\x1b[36;1m>_ Owly setup\x1b[0m");
    println!("Configure your inference provider and API key.");
    println!();

    let provider_labels: Vec<String> = provider_select_items().into_iter().map(|(_, label)| label).collect();
    let default_provider_idx = ONBOARDING_PROVIDERS
        .iter()
        .position(|id| *id == config.provider)
        .unwrap_or(0);

    let selection = Select::new()
        .with_prompt("Provider")
        .items(&provider_labels)
        .default(default_provider_idx)
        .interact()
        .context("provider selection cancelled")?;

    let provider = ONBOARDING_PROVIDERS[selection].to_string();
    let provider_cfg = provider_config(&provider).context("unknown provider")?;

    let api_key: String = Input::new()
        .with_prompt(format!("{} API key", provider_cfg.label))
        .interact_text()
        .context("api key input cancelled")?;

    let base_url = if setup_collects_base_url(&provider) {
        let prompt = base_url_label(&provider).unwrap_or_else(|| "Base URL".to_string());
        let value: String = Input::new().with_prompt(prompt).allow_empty(true).interact_text()?;
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    } else {
        None
    };

    let default_model = provider_cfg.default_model;
    let model_id: String = Input::new()
        .with_prompt("Model ID")
        .default(default_model.to_string())
        .interact_text()
        .context("model input cancelled")?;

    let next = apply_setup(
        SetupCredentials {
            provider,
            api_key,
            base_url,
            model_id,
        },
        config,
    )?;
    config.provider = next.provider;
    config.model_id = next.model_id;

    println!();
    println!("\x1b[32m✓\x1b[0m Credentials saved to {}", credentials::env_path().display());
    println!();

    Ok(())
}
