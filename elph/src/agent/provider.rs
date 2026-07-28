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
        "amazon-bedrock" => Some(ProviderConfig {
            label: "Amazon Bedrock",
            api_key_env_key: "AWS_ACCESS_KEY_ID",
            default_model: "amazon.nova-2-lite-v1:0",
        }),
        "ant-ling" => Some(ProviderConfig {
            label: "Ant Ling",
            api_key_env_key: "ANT_LING_API_KEY",
            default_model: "Ling-2.6-1T",
        }),
        "anthropic" => Some(ProviderConfig {
            label: "Anthropic",
            api_key_env_key: "ANTHROPIC_API_KEY",
            default_model: "claude-sonnet-4-20250514",
        }),
        "azure-openai-responses" => Some(ProviderConfig {
            label: "Azure OpenAI",
            api_key_env_key: "AZURE_OPENAI_API_KEY",
            default_model: "gpt-4",
        }),
        "cerebras" => Some(ProviderConfig {
            label: "Cerebras",
            api_key_env_key: "CEREBRAS_API_KEY",
            default_model: "gemma-4-31b",
        }),
        "cloudflare-ai-gateway" => Some(ProviderConfig {
            label: "Cloudflare AI Gateway",
            api_key_env_key: "CLOUDFLARE_API_KEY",
            default_model: "claude-3-5-haiku",
        }),
        "cloudflare-workers-ai" => Some(ProviderConfig {
            label: "Cloudflare Workers AI",
            api_key_env_key: "CLOUDFLARE_API_KEY",
            default_model: "@cf/google/gemma-4-26b-a4b-it",
        }),
        "deepseek" => Some(ProviderConfig {
            label: "DeepSeek",
            api_key_env_key: "DEEPSEEK_API_KEY",
            default_model: "deepseek-chat",
        }),
        "fireworks" => Some(ProviderConfig {
            label: "Fireworks",
            api_key_env_key: "FIREWORKS_API_KEY",
            default_model: "accounts/fireworks/models/deepseek-v4-flash",
        }),
        "github-copilot" => Some(ProviderConfig {
            label: "GitHub Copilot",
            api_key_env_key: "COPILOT_GITHUB_TOKEN",
            default_model: "claude-fable-5",
        }),
        "google" => Some(ProviderConfig {
            label: "Google",
            api_key_env_key: "GEMINI_API_KEY",
            default_model: "gemini-2.5-pro",
        }),
        "google-vertex" => Some(ProviderConfig {
            label: "Google Vertex AI",
            api_key_env_key: "GOOGLE_CLOUD_API_KEY",
            default_model: "gemini-2.5-flash",
        }),
        "groq" => Some(ProviderConfig {
            label: "Groq",
            api_key_env_key: "GROQ_API_KEY",
            default_model: "llama-3.1-8b-instant",
        }),
        "huggingface" => Some(ProviderConfig {
            label: "Hugging Face",
            api_key_env_key: "HF_TOKEN",
            default_model: "MiniMaxAI/MiniMax-M2",
        }),
        "hyper" => Some(ProviderConfig {
            label: "Charm Hyper",
            api_key_env_key: "HYPER_API_KEY",
            default_model: "deepseek-v4-flash",
        }),
        "kilo" => Some(ProviderConfig {
            label: "Kilo Gateway",
            api_key_env_key: "KILO_API_KEY",
            default_model: "kilo-auto/free",
        }),
        "kimi-coding" => Some(ProviderConfig {
            label: "Kimi For Coding",
            api_key_env_key: "MOONSHOT_API_KEY",
            default_model: "k3",
        }),
        "minimax" => Some(ProviderConfig {
            label: "MiniMax",
            api_key_env_key: "MINIMAX_API_KEY",
            default_model: "MiniMax-M2.7",
        }),
        "minimax-cn" => Some(ProviderConfig {
            label: "MiniMax (China)",
            api_key_env_key: "MINIMAX_API_KEY",
            default_model: "MiniMax-M2.7",
        }),
        "mistral" => Some(ProviderConfig {
            label: "Mistral",
            api_key_env_key: "MISTRAL_API_KEY",
            default_model: "codestral-latest",
        }),
        "moonshotai" => Some(ProviderConfig {
            label: "Moonshot AI",
            api_key_env_key: "MOONSHOT_API_KEY",
            default_model: "kimi-k2-0711-preview",
        }),
        "moonshotai-cn" => Some(ProviderConfig {
            label: "Moonshot AI (China)",
            api_key_env_key: "MOONSHOT_API_KEY",
            default_model: "kimi-k2-0711-preview",
        }),
        "neuralwatt" => Some(ProviderConfig {
            label: "Neuralwatt",
            api_key_env_key: "NEURALWATT_API_KEY",
            default_model: "deepseek-v4-flash",
        }),
        "nvidia" => Some(ProviderConfig {
            label: "NVIDIA NIM",
            api_key_env_key: "NVIDIA_API_KEY",
            default_model: "meta/llama-3.1-70b-instruct",
        }),
        "openai" => Some(ProviderConfig {
            label: "OpenAI",
            api_key_env_key: "OPENAI_API_KEY",
            default_model: "gpt-4.1",
        }),
        "openai-codex" => Some(ProviderConfig {
            label: "OpenAI Codex",
            api_key_env_key: "OPENAI_CODEX_OAUTH_TOKEN",
            default_model: "gpt-5.3-codex-spark",
        }),
        "opencode" => Some(ProviderConfig {
            label: "OpenCode Zen",
            api_key_env_key: "OPENCODE_API_KEY",
            default_model: "big-pickle",
        }),
        "opencode-go" => Some(ProviderConfig {
            label: "OpenCode Go",
            api_key_env_key: "OPENCODE_API_KEY",
            default_model: "deepseek-v4-flash",
        }),
        "opengateway" => Some(ProviderConfig {
            label: "OpenGateway",
            api_key_env_key: "OGW_API_KEY",
            default_model: "auto",
        }),
        "openrouter" => Some(ProviderConfig {
            label: "OpenRouter",
            api_key_env_key: "OPENROUTER_API_KEY",
            default_model: "anthropic/claude-sonnet-4",
        }),
        "qwen-token-plan" => Some(ProviderConfig {
            label: "Qwen Token Plan",
            api_key_env_key: "QWEN_TOKEN_PLAN_API_KEY",
            default_model: "qwen3.7-plus",
        }),
        "qwen-token-plan-cn" => Some(ProviderConfig {
            label: "Qwen Token Plan (China)",
            api_key_env_key: "QWEN_TOKEN_PLAN_CN_API_KEY",
            default_model: "qwen3.7-plus",
        }),
        "together" => Some(ProviderConfig {
            label: "Together AI",
            api_key_env_key: "TOGETHER_API_KEY",
            default_model: "MiniMaxAI/MiniMax-M2.7",
        }),
        "vercel-ai-gateway" => Some(ProviderConfig {
            label: "Vercel AI Gateway",
            api_key_env_key: "VERCEL_AI_GATEWAY_API_KEY",
            default_model: "alibaba/qwen-3-14b",
        }),
        "xai" => Some(ProviderConfig {
            label: "xAI",
            api_key_env_key: "XAI_API_KEY",
            default_model: "grok-3",
        }),
        "xiaomi" => Some(ProviderConfig {
            label: "Xiaomi MiMo",
            api_key_env_key: "XIAOMI_API_KEY",
            default_model: "mimo-v2-flash",
        }),
        "xiaomi-token-plan-ams" => Some(ProviderConfig {
            label: "Xiaomi Token Plan (AMS)",
            api_key_env_key: "XIAOMI_API_KEY",
            default_model: "mimo-v2-pro",
        }),
        "xiaomi-token-plan-cn" => Some(ProviderConfig {
            label: "Xiaomi Token Plan (CN)",
            api_key_env_key: "XIAOMI_API_KEY",
            default_model: "mimo-v2-pro",
        }),
        "xiaomi-token-plan-sgp" => Some(ProviderConfig {
            label: "Xiaomi Token Plan (SGP)",
            api_key_env_key: "XIAOMI_API_KEY",
            default_model: "mimo-v2-pro",
        }),
        "zai" => Some(ProviderConfig {
            label: "ZAI Coding Plan",
            api_key_env_key: "ZAI_API_KEY",
            default_model: "glm-4.5-air",
        }),
        "zai-coding-cn" => Some(ProviderConfig {
            label: "ZAI Coding Plan (China)",
            api_key_env_key: "ZAI_API_KEY",
            default_model: "glm-4.5-air",
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

    #[test]
    fn all_elph_ai_providers_are_known() {
        for id in elph_ai::get_builtin_providers() {
            let cfg = provider_config(id).unwrap_or_else(|| panic!("missing provider config for {id}"));
            assert!(!cfg.label.is_empty(), "empty label for {id}");
            assert!(!cfg.api_key_env_key.is_empty(), "empty api_key_env_key for {id}");
            assert!(!cfg.default_model.is_empty(), "empty default_model for {id}");
        }
    }

    #[test]
    fn every_provider_config_maps_to_elph_ai_provider() {
        // Spot-check a representative subset of newly added providers.
        let cases = [
            ("amazon-bedrock", "Amazon Bedrock", "AWS_ACCESS_KEY_ID"),
            ("ant-ling", "Ant Ling", "ANT_LING_API_KEY"),
            ("azure-openai-responses", "Azure OpenAI", "AZURE_OPENAI_API_KEY"),
            ("cerebras", "Cerebras", "CEREBRAS_API_KEY"),
            ("cloudflare-ai-gateway", "Cloudflare AI Gateway", "CLOUDFLARE_API_KEY"),
            ("cloudflare-workers-ai", "Cloudflare Workers AI", "CLOUDFLARE_API_KEY"),
            ("fireworks", "Fireworks", "FIREWORKS_API_KEY"),
            ("github-copilot", "GitHub Copilot", "COPILOT_GITHUB_TOKEN"),
            ("google-vertex", "Google Vertex AI", "GOOGLE_CLOUD_API_KEY"),
            ("groq", "Groq", "GROQ_API_KEY"),
            ("huggingface", "Hugging Face", "HF_TOKEN"),
            ("hyper", "Charm Hyper", "HYPER_API_KEY"),
            ("kimi-coding", "Kimi For Coding", "MOONSHOT_API_KEY"),
            ("minimax", "MiniMax", "MINIMAX_API_KEY"),
            ("minimax-cn", "MiniMax (China)", "MINIMAX_API_KEY"),
            ("mistral", "Mistral", "MISTRAL_API_KEY"),
            ("moonshotai", "Moonshot AI", "MOONSHOT_API_KEY"),
            ("moonshotai-cn", "Moonshot AI (China)", "MOONSHOT_API_KEY"),
            ("nvidia", "NVIDIA NIM", "NVIDIA_API_KEY"),
            ("openai-codex", "OpenAI Codex", "OPENAI_CODEX_OAUTH_TOKEN"),
            ("together", "Together AI", "TOGETHER_API_KEY"),
            ("vercel-ai-gateway", "Vercel AI Gateway", "VERCEL_AI_GATEWAY_API_KEY"),
            ("xai", "xAI", "XAI_API_KEY"),
            ("xiaomi", "Xiaomi MiMo", "XIAOMI_API_KEY"),
            ("xiaomi-token-plan-ams", "Xiaomi Token Plan (AMS)", "XIAOMI_API_KEY"),
            ("xiaomi-token-plan-cn", "Xiaomi Token Plan (CN)", "XIAOMI_API_KEY"),
            ("xiaomi-token-plan-sgp", "Xiaomi Token Plan (SGP)", "XIAOMI_API_KEY"),
            ("zai", "ZAI Coding Plan", "ZAI_API_KEY"),
            ("zai-coding-cn", "ZAI Coding Plan (China)", "ZAI_API_KEY"),
        ];
        for (id, label, env_key) in &cases {
            let cfg = provider_config(id).unwrap_or_else(|| panic!("unknown provider: {id}"));
            assert_eq!(cfg.label, *label, "label mismatch for {id}");
            assert_eq!(cfg.api_key_env_key, *env_key, "env key mismatch for {id}");
        }
    }

    #[test]
    fn resolve_new_provider_from_settings() {
        let (provider, model) =
            resolve_provider_and_model(None, None, Some("groq"), Some("llama-4.1-8b")).expect("resolve groq");
        assert_eq!(provider, "groq");
        assert_eq!(model, "llama-4.1-8b");
    }

    #[test]
    fn parse_new_provider_slash_model_override() {
        let (provider, model) = parse_model_override("mistral/codestral-5.1").expect("parse mistral");
        assert_eq!(provider, "mistral");
        assert_eq!(model, "codestral-5.1");
    }

    #[test]
    fn provider_default_models_are_not_empty() {
        let providers = [
            "amazon-bedrock",
            "ant-ling",
            "anthropic",
            "azure-openai-responses",
            "cerebras",
            "cloudflare-ai-gateway",
            "cloudflare-workers-ai",
            "deepseek",
            "fireworks",
            "github-copilot",
            "google",
            "google-vertex",
            "groq",
            "huggingface",
            "hyper",
            "kilo",
            "kimi-coding",
            "minimax",
            "minimax-cn",
            "mistral",
            "moonshotai",
            "moonshotai-cn",
            "neuralwatt",
            "nvidia",
            "openai",
            "openai-codex",
            "opencode",
            "opencode-go",
            "opengateway",
            "openrouter",
            "qwen-token-plan",
            "qwen-token-plan-cn",
            "together",
            "vercel-ai-gateway",
            "xai",
            "xiaomi",
            "xiaomi-token-plan-ams",
            "xiaomi-token-plan-cn",
            "xiaomi-token-plan-sgp",
            "zai",
            "zai-coding-cn",
        ];
        for id in &providers {
            let cfg = provider_config(id).unwrap_or_else(|| panic!("unknown: {id}"));
            assert!(!cfg.default_model.is_empty(), "empty default model for {id}");
        }
    }
}
