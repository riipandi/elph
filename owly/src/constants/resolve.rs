use super::{DEFAULT_MODEL_ID, DEFAULT_PROVIDER, OWLY_MODEL_ID_ENV_KEY, OWLY_PROVIDER_ENV_KEY, provider_config};

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
    if std::env::var("OPENCODE_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "opencode";
    }
    if std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "anthropic";
    }
    if std::env::var("OPENAI_API_KEY").ok().filter(|v| !v.is_empty()).is_some() {
        return "openai";
    }
    if std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "openrouter";
    }
    if std::env::var("GOOGLE_API_KEY").ok().filter(|v| !v.is_empty()).is_some() {
        return "google";
    }
    if std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
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

/// Normalize a model id from user input.
pub fn normalize_model_id(value: &str) -> String {
    value.trim().to_string()
}

/// Validate model id format (OpenWiki-compatible rules).
pub fn is_valid_model_id(value: &str) -> bool {
    let model_id = normalize_model_id(value);
    !model_id.is_empty()
        && model_id.len() <= 120
        && model_id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && !model_id.contains("://")
        && model_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '+' | '-'))
}

/// Resolve optional provider base URL from environment.
pub fn resolve_provider_base_url(provider: &str) -> Option<String> {
    let cfg = provider_config(provider)?;
    let key = cfg.base_url_env_key?;
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

pub fn provider_requires_base_url(provider: &str) -> bool {
    provider_config(provider).map(|c| c.requires_base_url).unwrap_or(false)
}
