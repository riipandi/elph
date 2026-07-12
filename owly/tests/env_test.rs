//! Tests for Owly env module.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `test/env.test.ts`. Original MIT License, Copyright (c) 2026 LangChain.

use owly::constants::*;

#[test]
fn test_provider_config_opencode() {
    let config = provider_config("opencode").unwrap();
    assert_eq!(config.label, "OpenCode Zen");
    assert_eq!(config.api_key_env_key, "OPENCODE_API_KEY");
    assert_eq!(config.default_model, "big-pickle");
}

#[test]
fn test_provider_config_anthropic() {
    let config = provider_config("anthropic").unwrap();
    assert_eq!(config.label, "Anthropic");
    assert_eq!(config.api_key_env_key, "ANTHROPIC_API_KEY");
    assert_eq!(config.default_model, "claude-haiku-4-5");
}

#[test]
fn test_provider_config_openai() {
    let config = provider_config("openai").unwrap();
    assert_eq!(config.label, "OpenAI");
    assert_eq!(config.api_key_env_key, "OPENAI_API_KEY");
    assert_eq!(config.default_model, "gpt-5.6-terra");
}

#[test]
fn test_provider_config_openrouter() {
    let config = provider_config("openrouter").unwrap();
    assert_eq!(config.label, "OpenRouter");
    assert_eq!(config.api_key_env_key, "OPENROUTER_API_KEY");
    assert_eq!(config.default_model, "z-ai/glm-5.2");
}

#[test]
fn test_provider_config_google() {
    let config = provider_config("google").unwrap();
    assert_eq!(config.label, "Google");
    assert_eq!(config.api_key_env_key, "GEMINI_API_KEY");
    assert_eq!(config.default_model, "gemini-2.5-flash");
}

#[test]
fn test_provider_config_deepseek() {
    let config = provider_config("deepseek").unwrap();
    assert_eq!(config.label, "DeepSeek");
    assert_eq!(config.api_key_env_key, "DEEPSEEK_API_KEY");
}

#[test]
fn test_provider_config_unknown() {
    assert!(provider_config("unknown-provider").is_none());
}

#[test]
fn test_all_providers_contains_expected() {
    let providers = all_providers();
    assert!(providers.contains(&"opencode"));
    assert!(providers.contains(&"anthropic"));
    assert!(providers.contains(&"openai"));
    assert!(providers.contains(&"openrouter"));
    assert!(providers.contains(&"google"));
    assert!(providers.contains(&"deepseek"));
    assert!(providers.contains(&"groq"));
    assert!(providers.contains(&"fireworks"));
    assert!(providers.contains(&"together"));
    assert!(providers.contains(&"mistral"));
}

#[test]
fn test_default_provider_is_opencode() {
    assert_eq!(DEFAULT_PROVIDER, "opencode");
}

#[test]
fn test_default_model_is_big_pickle() {
    assert_eq!(DEFAULT_MODEL_ID, "big-pickle");
}

#[test]
fn test_constants_values() {
    assert_eq!(OWLY_DIR, "openwiki");
    assert_eq!(UPDATE_METADATA_PATH, "openwiki/.last-update.json");
    assert_eq!(OWLY_VERSION, env!("CARGO_PKG_VERSION"));
}

#[test]
fn test_env_key_constants() {
    assert_eq!(OWLY_PROVIDER_ENV_KEY, "OWLY_PROVIDER");
    assert_eq!(OWLY_MODEL_ID_ENV_KEY, "OWLY_MODEL_ID");
}

#[test]
fn test_resolve_configured_provider_default() {
    // When no env vars are set, should return default provider
    let provider = resolve_configured_provider();
    assert!(!provider.is_empty());
}

#[test]
fn test_resolve_model_id_override() {
    // CLI override should take precedence
    let model = resolve_model_id(Some("custom-model"));
    assert_eq!(model, "custom-model");
}

#[test]
fn test_provider_needs_api_key() {
    // Without setting env vars, all providers need API keys
    // This test just checks the function doesn't panic
    let _ = provider_needs_api_key("opencode");
    let _ = provider_needs_api_key("anthropic");
    let _ = provider_needs_api_key("openai");
}

#[test]
fn test_resolve_provider_retry_attempts_default() {
    unsafe {
        std::env::remove_var(OWLY_PROVIDER_RETRY_ATTEMPTS_ENV_KEY);
    }
    assert_eq!(resolve_provider_retry_attempts().unwrap(), DEFAULT_PROVIDER_RETRY_ATTEMPTS);
}

#[test]
fn test_resolve_provider_retry_attempts_custom() {
    unsafe {
        std::env::set_var(OWLY_PROVIDER_RETRY_ATTEMPTS_ENV_KEY, "5");
    }
    assert_eq!(resolve_provider_retry_attempts().unwrap(), 5);
    unsafe {
        std::env::remove_var(OWLY_PROVIDER_RETRY_ATTEMPTS_ENV_KEY);
    }
}

#[test]
fn test_resolve_provider_retry_attempts_rejects_invalid() {
    unsafe {
        std::env::set_var(OWLY_PROVIDER_RETRY_ATTEMPTS_ENV_KEY, "0");
    }
    assert!(resolve_provider_retry_attempts().is_err());
    unsafe {
        std::env::set_var(OWLY_PROVIDER_RETRY_ATTEMPTS_ENV_KEY, "abc");
    }
    assert!(resolve_provider_retry_attempts().is_err());
    unsafe {
        std::env::remove_var(OWLY_PROVIDER_RETRY_ATTEMPTS_ENV_KEY);
    }
}
