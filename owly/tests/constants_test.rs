//! Tests for Owly constants module.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `test/constants.test.ts`. Original MIT License, Copyright (c) 2026 LangChain.
//!
//! Extended to test all providers from elph-ai.

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
fn test_provider_config_all_providers() {
    // Test that all providers have valid configs
    for provider in all_providers() {
        let config = provider_config(provider);
        assert!(config.is_some(), "Provider {} should have a config", provider);
        let config = config.unwrap();
        assert!(!config.label.is_empty(), "Provider {} should have a label", provider);
        if config.auth_method == ProviderAuthMethod::ApiKey {
            assert!(
                !config.api_key_env_key.is_empty(),
                "Provider {} should have an API key env key",
                provider
            );
        }
        assert!(
            !config.default_model.is_empty(),
            "Provider {} should have a default model",
            provider
        );
    }
}

#[test]
fn test_resolve_configured_provider_with_env() {
    // Test that OWLY_PROVIDER env var is honored
    // SAFETY: We're setting env vars in a single-threaded test context
    unsafe {
        std::env::set_var("OWLY_PROVIDER", "anthropic");
    }
    let provider = resolve_configured_provider();
    assert_eq!(provider, "anthropic");
    unsafe {
        std::env::remove_var("OWLY_PROVIDER");
    }
}

#[test]
fn test_resolve_model_id_env_var() {
    // Test that OWLY_MODEL_ID env var is honored
    // SAFETY: We're setting env vars in a single-threaded test context
    unsafe {
        std::env::set_var("OWLY_MODEL_ID", "custom-model");
    }
    let model = resolve_model_id(None);
    assert_eq!(model, "custom-model");
    unsafe {
        std::env::remove_var("OWLY_MODEL_ID");
    }
}
