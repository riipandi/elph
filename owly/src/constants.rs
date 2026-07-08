//! Constants for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/constants.ts`. Original MIT License, Copyright (c) 2026 LangChain.
//!
//! Extended to support all providers available in `elph-ai`.

/// The directory where documentation is stored
pub const OWLY_DIR: &str = "openwiki";

/// Path to the last update metadata file
pub const UPDATE_METADATA_PATH: &str = "openwiki/.last-update.json";

/// Owly version
pub const OWLY_VERSION: &str = "0.0.1";

/// Environment variable keys
pub const OWLY_PROVIDER_ENV_KEY: &str = "OWLY_PROVIDER";
pub const OWLY_MODEL_ID_ENV_KEY: &str = "OWLY_MODEL_ID";

/// Default provider
pub const DEFAULT_PROVIDER: &str = "opencode";

/// Default model ID (OpenCode Zen big-pickle)
pub const DEFAULT_MODEL_ID: &str = "big-pickle";

/// Provider configuration with API key environment variable
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub label: &'static str,
    pub api_key_env_key: &'static str,
    pub default_model: &'static str,
}

/// All supported providers from elph-ai
pub fn provider_config(provider: &str) -> Option<ProviderConfig> {
    match provider {
        "opencode" => Some(ProviderConfig {
            label: "OpenCode Zen",
            api_key_env_key: "OPENCODE_API_KEY",
            default_model: "big-pickle",
        }),
        "opencode-go" => Some(ProviderConfig {
            label: "OpenCode Go",
            api_key_env_key: "OPENCODE_API_KEY",
            default_model: "big-pickle",
        }),
        "anthropic" => Some(ProviderConfig {
            label: "Anthropic",
            api_key_env_key: "ANTHROPIC_API_KEY",
            default_model: "claude-sonnet-5",
        }),
        "openai" => Some(ProviderConfig {
            label: "OpenAI",
            api_key_env_key: "OPENAI_API_KEY",
            default_model: "gpt-5.4-mini",
        }),
        "openrouter" => Some(ProviderConfig {
            label: "OpenRouter",
            api_key_env_key: "OPENROUTER_API_KEY",
            default_model: "z-ai/glm-5.2",
        }),
        "google" => Some(ProviderConfig {
            label: "Google",
            api_key_env_key: "GOOGLE_API_KEY",
            default_model: "gemini-2.5-flash",
        }),
        "google-vertex" => Some(ProviderConfig {
            label: "Google Vertex",
            api_key_env_key: "GOOGLE_APPLICATION_CREDENTIALS",
            default_model: "gemini-2.5-flash",
        }),
        "deepseek" => Some(ProviderConfig {
            label: "DeepSeek",
            api_key_env_key: "DEEPSEEK_API_KEY",
            default_model: "deepseek-chat",
        }),
        "xai" => Some(ProviderConfig {
            label: "xAI",
            api_key_env_key: "XAI_API_KEY",
            default_model: "grok-2",
        }),
        "groq" => Some(ProviderConfig {
            label: "Groq",
            api_key_env_key: "GROQ_API_KEY",
            default_model: "llama-3.3-70b-versatile",
        }),
        "fireworks" => Some(ProviderConfig {
            label: "Fireworks",
            api_key_env_key: "FIREWORKS_API_KEY",
            default_model: "accounts/fireworks/models/glm-5p2",
        }),
        "together" => Some(ProviderConfig {
            label: "Together",
            api_key_env_key: "TOGETHER_API_KEY",
            default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        }),
        "mistral" => Some(ProviderConfig {
            label: "Mistral",
            api_key_env_key: "MISTRAL_API_KEY",
            default_model: "mistral-large-latest",
        }),
        "nvidia" => Some(ProviderConfig {
            label: "NVIDIA",
            api_key_env_key: "NVIDIA_API_KEY",
            default_model: "meta/llama-3.3-70b-instruct",
        }),
        "cerebras" => Some(ProviderConfig {
            label: "Cerebras",
            api_key_env_key: "CEREBRAS_API_KEY",
            default_model: "llama-3.3-70b",
        }),
        "amazon-bedrock" => Some(ProviderConfig {
            label: "Amazon Bedrock",
            api_key_env_key: "AWS_ACCESS_KEY_ID",
            default_model: "anthropic.claude-3-5-sonnet-20241022-v2:0",
        }),
        "github-copilot" => Some(ProviderConfig {
            label: "GitHub Copilot",
            api_key_env_key: "GITHUB_TOKEN",
            default_model: "gpt-4o",
        }),
        "cloudflare-workers-ai" => Some(ProviderConfig {
            label: "Cloudflare Workers AI",
            api_key_env_key: "CLOUDFLARE_API_TOKEN",
            default_model: "@cf/meta/llama-3.3-70b-instruct-fp8",
        }),
        "cloudflare-ai-gateway" => Some(ProviderConfig {
            label: "Cloudflare AI Gateway",
            api_key_env_key: "CLOUDFLARE_API_TOKEN",
            default_model: "@cf/meta/llama-3.3-70b-instruct-fp8",
        }),
        "huggingface" => Some(ProviderConfig {
            label: "Hugging Face",
            api_key_env_key: "HF_TOKEN",
            default_model: "meta-llama/Llama-3.3-70B-Instruct",
        }),
        "moonshotai" => Some(ProviderConfig {
            label: "MoonshotAI",
            api_key_env_key: "MOONSHOT_API_KEY",
            default_model: "moonshot-v1-auto",
        }),
        "zai" => Some(ProviderConfig {
            label: "Z.AI",
            api_key_env_key: "ZAI_API_KEY",
            default_model: "glm-5.2",
        }),
        "xiaomi" => Some(ProviderConfig {
            label: "Xiaomi",
            api_key_env_key: "XIAOMI_API_KEY",
            default_model: "MiLM-7B-Chat",
        }),
        "minimax" => Some(ProviderConfig {
            label: "MiniMax",
            api_key_env_key: "MINIMAX_API_KEY",
            default_model: "abab6.5s-chat",
        }),
        "ant-ling" => Some(ProviderConfig {
            label: "Ant Ling",
            api_key_env_key: "ANT_LING_API_KEY",
            default_model: "qwen-72b-chat",
        }),
        _ => None,
    }
}

/// Get all supported provider IDs
pub fn all_providers() -> Vec<&'static str> {
    vec![
        "opencode",
        "opencode-go",
        "anthropic",
        "openai",
        "openrouter",
        "google",
        "google-vertex",
        "deepseek",
        "xai",
        "groq",
        "fireworks",
        "together",
        "mistral",
        "nvidia",
        "cerebras",
        "amazon-bedrock",
        "github-copilot",
        "cloudflare-workers-ai",
        "cloudflare-ai-gateway",
        "huggingface",
        "moonshotai",
        "zai",
        "xiaomi",
        "minimax",
        "ant-ling",
    ]
}

/// Resolve provider from environment
pub fn resolve_configured_provider() -> &'static str {
    // Check OWLY_PROVIDER env var first
    if let Ok(provider) = std::env::var(OWLY_PROVIDER_ENV_KEY)
        && provider_config(&provider).is_some()
    {
        return Box::leak(provider.into_boxed_str());
    }

    // Auto-detect based on available API keys
    // Check if env var is set AND non-empty
    if std::env::var("OPENCODE_API_KEY").ok().filter(|v| !v.is_empty()).is_some() {
        return "opencode";
    }
    if std::env::var("ANTHROPIC_API_KEY").ok().filter(|v| !v.is_empty()).is_some() {
        return "anthropic";
    }
    if std::env::var("OPENAI_API_KEY").ok().filter(|v| !v.is_empty()).is_some() {
        return "openai";
    }
    if std::env::var("OPENROUTER_API_KEY").ok().filter(|v| !v.is_empty()).is_some() {
        return "openrouter";
    }
    if std::env::var("GOOGLE_API_KEY").ok().filter(|v| !v.is_empty()).is_some() {
        return "google";
    }
    if std::env::var("DEEPSEEK_API_KEY").ok().filter(|v| !v.is_empty()).is_some() {
        return "deepseek";
    }
    if std::env::var("GROQ_API_KEY").ok().filter(|v| !v.is_empty()).is_some() {
        return "groq";
    }

    DEFAULT_PROVIDER
}
/// Resolve model ID from environment or config
pub fn resolve_model_id(model_override: Option<&str>) -> String {
    // CLI override takes precedence
    if let Some(model) = model_override {
        return model.to_string();
    }

    // Check OWLY_MODEL_ID env var
    if let Ok(model) = std::env::var(OWLY_MODEL_ID_ENV_KEY)
        && !model.trim().is_empty()
    {
        return model;
    }

    // Use provider default
    let provider = resolve_configured_provider();
    provider_config(provider)
        .map(|c| c.default_model.to_string())
        .unwrap_or_else(|| DEFAULT_MODEL_ID.to_string())
}

/// Check if a provider needs an API key
pub fn provider_needs_api_key(provider: &str) -> bool {
    provider_config(provider)
        .map(|c| std::env::var(c.api_key_env_key).is_err())
        .unwrap_or(true)
}

/// Get API key for a provider
pub fn get_provider_api_key(provider: &str) -> Option<String> {
    provider_config(provider).and_then(|c| std::env::var(c.api_key_env_key).ok())
}
