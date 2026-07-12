use std::collections::HashMap;
use std::sync::LazyLock;

use elph_ai::{builtin_oauth_provider_ids, get_builtin_models, get_builtin_providers};

use super::{
    ANTHROPIC_BASE_URL_ENV_KEY, OPENAI_BASE_URL_ENV_KEY, OPENAI_COMPATIBLE_API_KEY_ENV_KEY,
    OPENAI_COMPATIBLE_BASE_URL_ENV_KEY, OPENROUTER_BASE_URL_ENV_KEY,
};

/// How a provider authenticates in Owly setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthMethod {
    ApiKey,
    OAuth,
}

/// Model option for onboarding wizards.
#[derive(Debug, Clone)]
pub struct ProviderModelOption {
    pub id: String,
    pub label: String,
}

/// Provider configuration backed by elph-ai catalogs.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub label: String,
    pub api_key_env_key: &'static str,
    pub default_model: String,
    pub base_url_env_key: Option<&'static str>,
    pub requires_base_url: bool,
    pub auth_method: ProviderAuthMethod,
}

struct ProviderMeta {
    label: &'static str,
    api_key_env_key: &'static str,
    base_url_env_key: Option<&'static str>,
    requires_base_url: bool,
}

static PROVIDER_METAS: LazyLock<HashMap<&'static str, ProviderMeta>> = LazyLock::new(|| {
    HashMap::from([
        (
            "opencode",
            ProviderMeta {
                label: "OpenCode Zen",
                api_key_env_key: "OPENCODE_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "opencode-go",
            ProviderMeta {
                label: "OpenCode Go",
                api_key_env_key: "OPENCODE_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "anthropic",
            ProviderMeta {
                label: "Anthropic",
                api_key_env_key: "ANTHROPIC_API_KEY",
                base_url_env_key: Some(ANTHROPIC_BASE_URL_ENV_KEY),
                requires_base_url: false,
            },
        ),
        (
            "openai",
            ProviderMeta {
                label: "OpenAI",
                api_key_env_key: "OPENAI_API_KEY",
                base_url_env_key: Some(OPENAI_BASE_URL_ENV_KEY),
                requires_base_url: false,
            },
        ),
        (
            "openai-codex",
            ProviderMeta {
                label: "OpenAI (ChatGPT login)",
                api_key_env_key: "",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "openrouter",
            ProviderMeta {
                label: "OpenRouter",
                api_key_env_key: "OPENROUTER_API_KEY",
                base_url_env_key: Some(OPENROUTER_BASE_URL_ENV_KEY),
                requires_base_url: false,
            },
        ),
        (
            "google",
            ProviderMeta {
                label: "Google",
                api_key_env_key: "GEMINI_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "google-vertex",
            ProviderMeta {
                label: "Google Vertex",
                api_key_env_key: "GOOGLE_APPLICATION_CREDENTIALS",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "deepseek",
            ProviderMeta {
                label: "DeepSeek",
                api_key_env_key: "DEEPSEEK_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "xai",
            ProviderMeta {
                label: "xAI",
                api_key_env_key: "XAI_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "groq",
            ProviderMeta {
                label: "Groq",
                api_key_env_key: "GROQ_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "fireworks",
            ProviderMeta {
                label: "Fireworks",
                api_key_env_key: "FIREWORKS_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "together",
            ProviderMeta {
                label: "Together",
                api_key_env_key: "TOGETHER_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "mistral",
            ProviderMeta {
                label: "Mistral",
                api_key_env_key: "MISTRAL_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "nvidia",
            ProviderMeta {
                label: "NVIDIA NIM",
                api_key_env_key: "NVIDIA_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "cerebras",
            ProviderMeta {
                label: "Cerebras",
                api_key_env_key: "CEREBRAS_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "amazon-bedrock",
            ProviderMeta {
                label: "Amazon Bedrock",
                api_key_env_key: "AWS_ACCESS_KEY_ID",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "github-copilot",
            ProviderMeta {
                label: "GitHub Copilot",
                api_key_env_key: "GITHUB_TOKEN",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "cloudflare-workers-ai",
            ProviderMeta {
                label: "Cloudflare Workers AI",
                api_key_env_key: "CLOUDFLARE_API_TOKEN",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "cloudflare-ai-gateway",
            ProviderMeta {
                label: "Cloudflare AI Gateway",
                api_key_env_key: "CLOUDFLARE_API_TOKEN",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "huggingface",
            ProviderMeta {
                label: "Hugging Face",
                api_key_env_key: "HF_TOKEN",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "moonshotai",
            ProviderMeta {
                label: "MoonshotAI",
                api_key_env_key: "MOONSHOT_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "zai",
            ProviderMeta {
                label: "Z.AI",
                api_key_env_key: "ZAI_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "xiaomi",
            ProviderMeta {
                label: "Xiaomi",
                api_key_env_key: "XIAOMI_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "minimax",
            ProviderMeta {
                label: "MiniMax",
                api_key_env_key: "MINIMAX_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "ant-ling",
            ProviderMeta {
                label: "Ant Ling",
                api_key_env_key: "ANT_LING_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "azure-openai-responses",
            ProviderMeta {
                label: "Azure OpenAI",
                api_key_env_key: "AZURE_OPENAI_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "hyper",
            ProviderMeta {
                label: "Charm Hyper",
                api_key_env_key: "HYPER_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "kimi-coding",
            ProviderMeta {
                label: "Kimi Coding",
                api_key_env_key: "MOONSHOT_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "minimax-cn",
            ProviderMeta {
                label: "MiniMax (China)",
                api_key_env_key: "MINIMAX_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "moonshotai-cn",
            ProviderMeta {
                label: "MoonshotAI (China)",
                api_key_env_key: "MOONSHOT_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "vercel-ai-gateway",
            ProviderMeta {
                label: "Vercel AI Gateway",
                api_key_env_key: "VERCEL_AI_GATEWAY_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "xiaomi-token-plan-ams",
            ProviderMeta {
                label: "Xiaomi Token Plan (AMS)",
                api_key_env_key: "XIAOMI_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "xiaomi-token-plan-cn",
            ProviderMeta {
                label: "Xiaomi Token Plan (CN)",
                api_key_env_key: "XIAOMI_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "xiaomi-token-plan-sgp",
            ProviderMeta {
                label: "Xiaomi Token Plan (SGP)",
                api_key_env_key: "XIAOMI_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
        (
            "zai-coding-cn",
            ProviderMeta {
                label: "Z.AI Coding (CN)",
                api_key_env_key: "ZAI_API_KEY",
                base_url_env_key: None,
                requires_base_url: false,
            },
        ),
    ])
});

const OPENAI_COMPATIBLE_META: ProviderMeta = ProviderMeta {
    label: "OpenAI-compatible",
    api_key_env_key: OPENAI_COMPATIBLE_API_KEY_ENV_KEY,
    base_url_env_key: Some(OPENAI_COMPATIBLE_BASE_URL_ENV_KEY),
    requires_base_url: true,
};

/// Curated wizard model lists (aligned with OpenWiki defaults; ids must exist in elph-ai catalogs).
fn curated_model_options(provider: &str) -> Option<Vec<(&'static str, &'static str)>> {
    match provider {
        "openai" | "openai-codex" => Some(vec![
            ("gpt-5.6-terra", "5.6 Terra"),
            ("gpt-5.6-luna", "5.6 Luna"),
            ("gpt-5.6-sol", "5.6 Sol"),
            ("gpt-5.5", "5.5"),
            ("gpt-5.4-mini", "5.4 mini"),
        ]),
        "anthropic" => Some(vec![
            ("claude-haiku-4-5", "Haiku"),
            ("claude-sonnet-5", "Sonnet"),
            ("claude-opus-4-8", "Opus"),
        ]),
        "openrouter" => Some(vec![
            ("z-ai/glm-5.2", "GLM 5.2"),
            ("openrouter/fusion", "OpenRouter Fusion"),
            ("moonshotai/kimi-k2.7-code", "Kimi K2.7 Code"),
            ("anthropic/claude-opus-4-8", "Claude Opus"),
            ("anthropic/claude-sonnet-5", "Claude Sonnet"),
            ("openai/gpt-5.4-mini", "GPT 5.4 mini"),
            ("openai/gpt-5.5", "GPT 5.5"),
        ]),
        "fireworks" => Some(vec![
            ("accounts/fireworks/models/glm-5p2", "GLM 5.2"),
            ("accounts/fireworks/models/kimi-k2p7-code", "Kimi K2.7 Code"),
        ]),
        "nvidia" => Some(vec![
            ("nvidia/nemotron-3-super-120b-a12b", "Nemotron 3 Super 120B"),
            ("nvidia/nemotron-3-ultra-550b-a55b", "Nemotron 3 Ultra 550B"),
            ("nvidia/nemotron-3-nano-omni-30b-a3b-reasoning", "Nemotron 3 Nano Omni 30B"),
            ("deepseek-ai/deepseek-v4-pro", "DeepSeek V4 Pro"),
            ("openai/gpt-oss-120b", "GPT-OSS 120B"),
            ("moonshotai/kimi-k2.6", "Kimi K2.6"),
        ]),
        "opencode" | "opencode-go" => Some(vec![("big-pickle", "big-pickle")]),
        "google" => Some(vec![
            ("gemini-2.5-flash", "Gemini 2.5 Flash"),
            ("gemini-2.5-pro", "Gemini 2.5 Pro"),
        ]),
        _ => None,
    }
}

fn preferred_default_model(provider: &str) -> Option<&'static str> {
    curated_model_options(provider).and_then(|opts| opts.first().map(|(id, _)| *id))
}

fn fallback_default_model(provider: &str) -> Option<String> {
    get_builtin_models(provider).first().map(|m| m.id.clone())
}

fn provider_label(_provider: &str, meta: &ProviderMeta) -> String {
    meta.label.to_string()
}

/// Returns true when `provider` is known to Owly (elph-ai builtin or Owly-only extension).
pub fn is_known_provider(provider: &str) -> bool {
    provider == "openai-compatible" || get_builtin_providers().contains(&provider)
}

/// Returns true when elph-ai supports OAuth for this provider.
pub fn provider_oauth_capable(provider: &str) -> bool {
    builtin_oauth_provider_ids().contains(&provider)
}

/// Returns true when setup must use OAuth (no API key path). Maps upstream `openai-chatgpt`.
pub fn provider_oauth_only(provider: &str) -> bool {
    provider == "openai-codex"
}

/// Alias for [`provider_oauth_capable`] (auth resolution may use OAuth tokens when present).
pub fn provider_uses_oauth(provider: &str) -> bool {
    provider_oauth_capable(provider)
}

/// Model options for onboarding (curated when available, otherwise elph-ai catalog).
pub fn provider_models_for_wizard(provider: &str) -> Vec<ProviderModelOption> {
    if let Some(curated) = curated_model_options(provider) {
        let catalog_ids: std::collections::HashSet<String> =
            get_builtin_models(provider).into_iter().map(|m| m.id).collect();
        return curated
            .into_iter()
            .filter(|(id, _)| catalog_ids.is_empty() || catalog_ids.contains(*id))
            .map(|(id, label)| ProviderModelOption {
                id: id.to_string(),
                label: label.to_string(),
            })
            .collect();
    }

    get_builtin_models(provider)
        .into_iter()
        .map(|m| ProviderModelOption {
            id: m.id.clone(),
            label: m.name,
        })
        .collect()
}

/// All supported provider IDs (elph-ai builtins + Owly-only `openai-compatible`).
pub fn all_providers() -> Vec<&'static str> {
    let mut providers = get_builtin_providers();
    if !providers.contains(&"openai-compatible") {
        providers.push("openai-compatible");
    }
    providers.sort_unstable();
    providers
}

pub fn provider_config(provider: &str) -> Option<ProviderConfig> {
    if !is_known_provider(provider) {
        return None;
    }

    let meta = if provider == "openai-compatible" {
        &OPENAI_COMPATIBLE_META
    } else {
        PROVIDER_METAS.get(provider)?
    };

    let auth_method = if provider_oauth_only(provider) {
        ProviderAuthMethod::OAuth
    } else {
        ProviderAuthMethod::ApiKey
    };

    let default_model = if provider == "openai-compatible" {
        "gpt-4o-mini".to_string()
    } else {
        preferred_default_model(provider)
            .map(str::to_string)
            .or_else(|| fallback_default_model(provider))
            .unwrap_or_default()
    };

    Some(ProviderConfig {
        label: provider_label(provider, meta),
        api_key_env_key: meta.api_key_env_key,
        default_model,
        base_url_env_key: meta.base_url_env_key,
        requires_base_url: meta.requires_base_url,
        auth_method,
    })
}
