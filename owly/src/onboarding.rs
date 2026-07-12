//! First-run credential onboarding wizard.

use anyhow::{Context, Result};
use dialoguer::{Input, Select};
use std::collections::HashMap;

use crate::agent::credential_store;
use crate::config::Config;
use crate::constants::{
    ONBOARDING_PROVIDERS, OWLY_MODEL_ID_ENV_KEY, OWLY_PROVIDER_ENV_KEY, is_valid_model_id, normalize_model_id,
    provider_config, provider_is_configured, provider_models_for_wizard, provider_oauth_capable, provider_oauth_only,
    provider_requires_base_url, resolve_provider_base_url,
};
use crate::credentials::{self, run_oauth_login, save_env};

/// Credentials collected during interactive setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupCredentials {
    pub provider: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model_id: String,
    /// When true, OAuth was completed during setup (no API key env var).
    pub oauth: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupAuthChoice {
    ApiKey,
    OAuth,
}

/// Returns true when interactive setup should run before the first agent call.
pub fn needs_setup(config: &Config) -> bool {
    !provider_is_configured(&config.provider)
        || (provider_requires_base_url(&config.provider) && resolve_provider_base_url(&config.provider).is_none())
}

/// Validate, persist, and return updated config fields after setup.
pub fn apply_setup(credentials: SetupCredentials, config: &Config) -> Result<Config> {
    let provider = credentials.provider.trim();
    let provider_cfg = provider_config(provider).with_context(|| format!("Unknown provider: {provider}"))?;

    let model_id = normalize_model_id(&credentials.model_id);
    if !is_valid_model_id(&model_id) {
        anyhow::bail!("Invalid model ID: {model_id}");
    }

    let mut updates = HashMap::from([
        (OWLY_PROVIDER_ENV_KEY.to_string(), provider.to_string()),
        (OWLY_MODEL_ID_ENV_KEY.to_string(), model_id.clone()),
    ]);

    if !credentials.oauth {
        let api_key = credentials.api_key.trim();
        if api_key.is_empty() {
            anyhow::bail!("{} API key is required.", provider_cfg.label);
        }
        updates.insert(provider_cfg.api_key_env_key.to_string(), api_key.to_string());
    }

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
    provider_config(provider).map(|cfg| cfg.default_model.clone())
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

/// Whether setup must run OAuth (no API key step). Only `openai-codex` today.
pub fn setup_uses_oauth(provider: &str) -> bool {
    provider_oauth_only(provider)
}

/// Run elph-ai OAuth login and persist tokens for `provider`.
pub fn run_provider_oauth_login(provider: &str) -> Result<()> {
    let store = credential_store();
    tokio::runtime::Handle::current()
        .block_on(run_oauth_login(provider, store.as_ref()))
        .with_context(|| format!("OAuth login failed for {provider}"))
}

/// API key env label for prompts.
pub fn api_key_label(provider: &str) -> Option<String> {
    if setup_uses_oauth(provider) {
        return None;
    }
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

fn prompt_setup_auth(provider: &str) -> Result<SetupAuthChoice> {
    if setup_uses_oauth(provider) {
        return Ok(SetupAuthChoice::OAuth);
    }
    if provider_oauth_capable(provider) {
        let options = ["API key", "Sign in with browser (OAuth)"];
        let idx = Select::new()
            .with_prompt("Authentication")
            .items(options)
            .default(0)
            .interact()
            .context("authentication selection cancelled")?;
        return Ok(if idx == 0 {
            SetupAuthChoice::ApiKey
        } else {
            SetupAuthChoice::OAuth
        });
    }
    Ok(SetupAuthChoice::ApiKey)
}

fn select_model_id(provider: &str, default_model: &str) -> Result<String> {
    let options = provider_models_for_wizard(provider);
    if options.is_empty() {
        return Input::new()
            .with_prompt("Model ID")
            .default(default_model.to_string())
            .interact_text()
            .context("model input cancelled");
    }

    let labels: Vec<String> = options
        .iter()
        .map(|opt| {
            if opt.label == opt.id {
                opt.id.clone()
            } else {
                format!("{} ({})", opt.label, opt.id)
            }
        })
        .collect();
    let default_idx = options.iter().position(|opt| opt.id == default_model).unwrap_or(0);

    let selection = Select::new()
        .with_prompt("Model")
        .items(&labels)
        .default(default_idx)
        .interact()
        .context("model selection cancelled")?;

    Ok(options[selection].id.clone())
}

fn prompt_base_url(provider: &str) -> Result<Option<String>> {
    if !setup_collects_base_url(provider) {
        return Ok(None);
    }
    let prompt = base_url_label(provider).unwrap_or_else(|| "Base URL".to_string());
    let value: String = Input::new().with_prompt(prompt).allow_empty(true).interact_text()?;
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed))
    }
}

fn finish_setup(
    config: &mut Config,
    provider: String,
    api_key: String,
    base_url: Option<String>,
    model_id: String,
    oauth: bool,
) -> Result<()> {
    let next = apply_setup(
        SetupCredentials {
            provider,
            api_key,
            base_url,
            model_id,
            oauth,
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

/// Run the interactive credential wizard and persist settings to `~/.owly/.env`.
pub fn run_wizard(config: &mut Config) -> Result<()> {
    println!();
    println!("\x1b[36;1m>_ Owly setup\x1b[0m");
    println!("Configure your inference provider and credentials.");
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

    let auth = prompt_setup_auth(&provider)?;
    let oauth = auth == SetupAuthChoice::OAuth;

    if oauth {
        println!();
        println!("Sign in with {} (browser flow).", provider_cfg.label);
        run_provider_oauth_login(&provider)?;
        println!("\x1b[32m✓\x1b[0m Signed in.");
    } else {
        let api_key: String = Input::new()
            .with_prompt(format!("{} API key", provider_cfg.label))
            .interact_text()
            .context("api key input cancelled")?;

        let base_url = prompt_base_url(&provider)?;
        let model_id = select_model_id(&provider, &provider_cfg.default_model)?;

        return finish_setup(config, provider, api_key, base_url, model_id, false);
    }

    let base_url = prompt_base_url(&provider)?;
    let model_id = select_model_id(&provider, &provider_cfg.default_model)?;
    finish_setup(config, provider, String::new(), base_url, model_id, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::ProviderAuthMethod;

    #[test]
    fn openai_codex_is_oauth_only_setup() {
        assert!(setup_uses_oauth("openai-codex"));
        assert!(provider_oauth_only("openai-codex"));
        assert_eq!(provider_config("openai-codex").unwrap().auth_method, ProviderAuthMethod::OAuth);
    }

    #[test]
    fn anthropic_defaults_to_api_key_setup() {
        assert!(!setup_uses_oauth("anthropic"));
        assert!(provider_oauth_capable("anthropic"));
        assert_eq!(provider_config("anthropic").unwrap().auth_method, ProviderAuthMethod::ApiKey);
    }
}
