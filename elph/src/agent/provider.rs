//! Provider resolution for the Elph coding agent.

use anyhow::{Context, Result};

pub const DEFAULT_PROVIDER: &str = "opencode";
pub const DEFAULT_MODEL_ID: &str = "big-pickle";

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub label: &'static str,
    pub api_key_env_key: &'static str,
    pub default_model: &'static str,
}

pub fn provider_api_key_env(provider: &str) -> Option<&'static str> {
    provider_config(provider).map(|c| c.api_key_env_key)
}

pub fn provider_config(provider: &str) -> Option<ProviderConfig> {
    match provider {
        "opencode" => Some(ProviderConfig {
            label: "OpenCode Zen",
            api_key_env_key: "OPENCODE_API_KEY",
            default_model: "big-pickle",
        }),
        "anthropic" => Some(ProviderConfig {
            label: "Anthropic",
            api_key_env_key: "ANTHROPIC_API_KEY",
            default_model: "claude-sonnet-4-20250514",
        }),
        "openai" => Some(ProviderConfig {
            label: "OpenAI",
            api_key_env_key: "OPENAI_API_KEY",
            default_model: "gpt-4.1",
        }),
        "openrouter" => Some(ProviderConfig {
            label: "OpenRouter",
            api_key_env_key: "OPENROUTER_API_KEY",
            default_model: "anthropic/claude-sonnet-4",
        }),
        // Gitlawb OpenGateway — https://gitlawb.com/opengateway (OGW_API_KEY | OPENGATEWAY_API_KEY).
        "opengateway" => Some(ProviderConfig {
            label: "OpenGateway",
            api_key_env_key: "OGW_API_KEY",
            default_model: "auto",
        }),
        // Kilo AI Gateway — https://kilo.ai/docs/gateway (KILO_API_KEY).
        "kilo" => Some(ProviderConfig {
            label: "Kilo Gateway",
            api_key_env_key: "KILO_API_KEY",
            default_model: "kilo-auto/free",
        }),
        "google" => Some(ProviderConfig {
            label: "Google",
            api_key_env_key: "GOOGLE_API_KEY",
            default_model: "gemini-2.5-pro",
        }),
        "deepseek" => Some(ProviderConfig {
            label: "DeepSeek",
            api_key_env_key: "DEEPSEEK_API_KEY",
            default_model: "deepseek-chat",
        }),
        _ => None,
    }
}

pub fn resolve_configured_provider() -> &'static str {
    DEFAULT_PROVIDER
}

pub fn resolve_model_id_for_provider(provider: &str) -> String {
    provider_config(provider)
        .map(|c| c.default_model.to_string())
        .unwrap_or_else(|| DEFAULT_MODEL_ID.to_string())
}

pub fn parse_model_override(value: &str) -> Option<(String, String)> {
    let (provider, model) = value.split_once('/')?;
    provider_config(provider).map(|_| (provider.to_string(), model.to_string()))
}

pub fn resolve_provider_and_model(
    provider_override: Option<&str>,
    model_override: Option<&str>,
    settings_provider: Option<&str>,
    settings_model: Option<&str>,
) -> Result<(String, String)> {
    if let Some(value) = model_override
        && let Some((provider, model)) = parse_model_override(value)
    {
        provider_config(&provider).with_context(|| format!("Unknown provider: {provider}"))?;
        return Ok((provider, model));
    }

    let provider = provider_override
        .map(str::to_string)
        .or_else(|| std::env::var("ELPH_PROVIDER").ok())
        .or_else(|| settings_provider.map(str::to_string))
        .unwrap_or_else(|| resolve_configured_provider().to_string());

    provider_config(&provider).with_context(|| format!("Unknown provider: {provider}"))?;

    let model_id = model_override
        .map(str::to_string)
        .or_else(|| std::env::var("ELPH_MODEL").ok())
        .or_else(|| settings_model.map(str::to_string))
        .unwrap_or_else(|| resolve_model_id_for_provider(&provider));

    Ok((provider, model_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opengateway_is_a_known_provider() {
        let cfg = provider_config("opengateway").expect("opengateway config");
        assert_eq!(cfg.label, "OpenGateway");
        assert_eq!(cfg.api_key_env_key, "OGW_API_KEY");
        assert_eq!(cfg.default_model, "auto");
    }

    #[test]
    fn resolve_opengateway_from_settings() {
        let (provider, model) =
            resolve_provider_and_model(None, None, Some("opengateway"), Some("nvidia/nemotron-3-ultra-550b-a55b:free"))
                .expect("resolve");
        assert_eq!(provider, "opengateway");
        assert_eq!(model, "nvidia/nemotron-3-ultra-550b-a55b:free");
    }

    #[test]
    fn parse_opengateway_slash_model_override() {
        let (provider, model) = parse_model_override("opengateway/xiaomi/mimo-v2.5-pro").expect("parse");
        assert_eq!(provider, "opengateway");
        assert_eq!(model, "xiaomi/mimo-v2.5-pro");
    }

    #[test]
    fn kilo_is_a_known_provider() {
        let cfg = provider_config("kilo").expect("kilo config");
        assert_eq!(cfg.label, "Kilo Gateway");
        assert_eq!(cfg.api_key_env_key, "KILO_API_KEY");
        assert_eq!(cfg.default_model, "kilo-auto/free");
    }

    #[test]
    fn resolve_kilo_from_settings() {
        let (provider, model) =
            resolve_provider_and_model(None, None, Some("kilo"), Some("kilo-auto/frontier")).expect("resolve");
        assert_eq!(provider, "kilo");
        assert_eq!(model, "kilo-auto/frontier");
    }

    #[test]
    fn parse_kilo_slash_model_override() {
        let (provider, model) = parse_model_override("kilo/anthropic/claude-sonnet-4.6").expect("parse");
        assert_eq!(provider, "kilo");
        assert_eq!(model, "anthropic/claude-sonnet-4.6");
    }
}
