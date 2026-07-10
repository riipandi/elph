use super::{
    ANTHROPIC_BASE_URL_ENV_KEY, OPENAI_BASE_URL_ENV_KEY, OPENAI_COMPATIBLE_API_KEY_ENV_KEY,
    OPENAI_COMPATIBLE_BASE_URL_ENV_KEY, OPENROUTER_BASE_URL_ENV_KEY,
};

/// Provider configuration with API key environment variable
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub label: &'static str,
    pub api_key_env_key: &'static str,
    pub default_model: &'static str,
    pub base_url_env_key: Option<&'static str>,
    pub requires_base_url: bool,
}

const fn provider_defaults(
    label: &'static str,
    api_key_env_key: &'static str,
    default_model: &'static str,
) -> ProviderConfig {
    ProviderConfig {
        label,
        api_key_env_key,
        default_model,
        base_url_env_key: None,
        requires_base_url: false,
    }
}

/// All supported providers from elph-ai
pub fn provider_config(provider: &str) -> Option<ProviderConfig> {
    match provider {
        "opencode" => Some(ProviderConfig {
            label: "OpenCode Zen",
            api_key_env_key: "OPENCODE_API_KEY",
            default_model: "big-pickle",
            base_url_env_key: None,
            requires_base_url: false,
        }),
        "opencode-go" => Some(ProviderConfig {
            label: "OpenCode Go",
            api_key_env_key: "OPENCODE_API_KEY",
            default_model: "big-pickle",
            base_url_env_key: None,
            requires_base_url: false,
        }),
        "anthropic" => Some(ProviderConfig {
            label: "Anthropic",
            api_key_env_key: "ANTHROPIC_API_KEY",
            default_model: "claude-sonnet-5",
            base_url_env_key: Some(ANTHROPIC_BASE_URL_ENV_KEY),
            requires_base_url: false,
        }),
        "openai" => Some(ProviderConfig {
            label: "OpenAI",
            api_key_env_key: "OPENAI_API_KEY",
            default_model: "gpt-5.4-mini",
            base_url_env_key: Some(OPENAI_BASE_URL_ENV_KEY),
            requires_base_url: false,
        }),
        "openai-compatible" => Some(ProviderConfig {
            label: "OpenAI-compatible",
            api_key_env_key: OPENAI_COMPATIBLE_API_KEY_ENV_KEY,
            default_model: "gpt-4o-mini",
            base_url_env_key: Some(OPENAI_COMPATIBLE_BASE_URL_ENV_KEY),
            requires_base_url: true,
        }),
        "openrouter" => Some(ProviderConfig {
            label: "OpenRouter",
            api_key_env_key: "OPENROUTER_API_KEY",
            default_model: "z-ai/glm-5.2",
            base_url_env_key: Some(OPENROUTER_BASE_URL_ENV_KEY),
            requires_base_url: false,
        }),
        "google" => Some(ProviderConfig {
            label: "Google",
            api_key_env_key: "GOOGLE_API_KEY",
            default_model: "gemini-2.5-flash",
            base_url_env_key: None,
            requires_base_url: false,
        }),
        "google-vertex" => Some(provider_defaults(
            "Google Vertex",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "gemini-2.5-flash",
        )),
        "deepseek" => Some(provider_defaults("DeepSeek", "DEEPSEEK_API_KEY", "deepseek-chat")),
        "xai" => Some(provider_defaults("xAI", "XAI_API_KEY", "grok-2")),
        "groq" => Some(provider_defaults("Groq", "GROQ_API_KEY", "llama-3.3-70b-versatile")),
        "fireworks" => Some(provider_defaults(
            "Fireworks",
            "FIREWORKS_API_KEY",
            "accounts/fireworks/models/glm-5p2",
        )),
        "together" => Some(provider_defaults(
            "Together",
            "TOGETHER_API_KEY",
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        )),
        "mistral" => Some(provider_defaults("Mistral", "MISTRAL_API_KEY", "mistral-large-latest")),
        "nvidia" => Some(provider_defaults(
            "NVIDIA",
            "NVIDIA_API_KEY",
            "meta/llama-3.3-70b-instruct",
        )),
        "cerebras" => Some(provider_defaults("Cerebras", "CEREBRAS_API_KEY", "llama-3.3-70b")),
        "amazon-bedrock" => Some(provider_defaults(
            "Amazon Bedrock",
            "AWS_ACCESS_KEY_ID",
            "anthropic.claude-3-5-sonnet-20241022-v2:0",
        )),
        "github-copilot" => Some(provider_defaults("GitHub Copilot", "GITHUB_TOKEN", "gpt-4o")),
        "cloudflare-workers-ai" => Some(provider_defaults(
            "Cloudflare Workers AI",
            "CLOUDFLARE_API_TOKEN",
            "@cf/meta/llama-3.3-70b-instruct-fp8",
        )),
        "cloudflare-ai-gateway" => Some(provider_defaults(
            "Cloudflare AI Gateway",
            "CLOUDFLARE_API_TOKEN",
            "@cf/meta/llama-3.3-70b-instruct-fp8",
        )),
        "huggingface" => Some(provider_defaults(
            "Hugging Face",
            "HF_TOKEN",
            "meta-llama/Llama-3.3-70B-Instruct",
        )),
        "moonshotai" => Some(provider_defaults("MoonshotAI", "MOONSHOT_API_KEY", "moonshot-v1-auto")),
        "zai" => Some(provider_defaults("Z.AI", "ZAI_API_KEY", "glm-5.2")),
        "xiaomi" => Some(provider_defaults("Xiaomi", "XIAOMI_API_KEY", "MiLM-7B-Chat")),
        "minimax" => Some(provider_defaults("MiniMax", "MINIMAX_API_KEY", "abab6.5s-chat")),
        "ant-ling" => Some(provider_defaults("Ant Ling", "ANT_LING_API_KEY", "qwen-72b-chat")),
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
        "openai-compatible",
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
