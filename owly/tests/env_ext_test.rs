//! Extended tests for Owly env module.

use owly::constants::*;

#[test]
fn test_provider_config_all_labels() {
    let test_cases = vec![
        ("opencode", "OpenCode Zen"),
        ("anthropic", "Anthropic"),
        ("openai", "OpenAI"),
        ("openrouter", "OpenRouter"),
        ("google", "Google"),
        ("deepseek", "DeepSeek"),
        ("groq", "Groq"),
        ("fireworks", "Fireworks"),
        ("together", "Together"),
        ("mistral", "Mistral"),
    ];

    for (provider, expected_label) in test_cases {
        let config = provider_config(provider).unwrap();
        assert_eq!(config.label, expected_label, "Provider: {}", provider);
    }
}

#[test]
fn test_provider_config_api_key_env_keys() {
    let test_cases = vec![
        ("opencode", "OPENCODE_API_KEY"),
        ("anthropic", "ANTHROPIC_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("openrouter", "OPENROUTER_API_KEY"),
        ("google", "GOOGLE_API_KEY"),
        ("deepseek", "DEEPSEEK_API_KEY"),
        ("groq", "GROQ_API_KEY"),
        ("fireworks", "FIREWORKS_API_KEY"),
        ("together", "TOGETHER_API_KEY"),
        ("mistral", "MISTRAL_API_KEY"),
    ];

    for (provider, expected_env_key) in test_cases {
        let config = provider_config(provider).unwrap();
        assert_eq!(config.api_key_env_key, expected_env_key, "Provider: {}", provider);
    }
}

#[test]
fn test_provider_config_default_models() {
    let test_cases = vec![
        ("opencode", "big-pickle"),
        ("anthropic", "claude-sonnet-5"),
        ("openai", "gpt-5.4-mini"),
    ];

    for (provider, expected_model) in test_cases {
        let config = provider_config(provider).unwrap();
        assert_eq!(config.default_model, expected_model, "Provider: {}", provider);
    }
}

#[test]
fn test_all_providers_have_valid_config() {
    for provider in all_providers() {
        let config = provider_config(provider);
        assert!(config.is_some(), "Provider {} should have a config", provider);
        let config = config.unwrap();
        assert!(!config.label.is_empty());
        assert!(!config.api_key_env_key.is_empty());
        assert!(!config.default_model.is_empty());
    }
}

#[test]
fn test_resolve_configured_provider_with_openrouter_key() {
    // SAFETY: We're setting env vars in a single-threaded test context
    unsafe {
        std::env::remove_var("OPENCODE_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::set_var("OPENROUTER_API_KEY", "test-key");
    }
    let provider = resolve_configured_provider();
    assert_eq!(provider, "openrouter");
    unsafe {
        std::env::remove_var("OPENROUTER_API_KEY");
    }
}

#[test]
fn test_resolve_configured_provider_with_anthropic_key() {
    // SAFETY: We're setting env vars in a single-threaded test context
    // The function checks OPENCODE_API_KEY first, so we need to remove it
    // to allow ANTHROPIC_API_KEY to win
    unsafe {
        // Remove all provider env vars first
        std::env::remove_var("OPENCODE_API_KEY");
        std::env::remove_var("OPENROUTER_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");
        // Now set ANTHROPIC_API_KEY
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    }
    let provider = resolve_configured_provider();
    assert_eq!(provider, "anthropic");
    // Restore
    unsafe {
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}

#[test]
fn test_resolve_model_id_with_env_var() {
    // SAFETY: We're setting env vars in a single-threaded test context
    unsafe {
        std::env::set_var("OWLY_MODEL_ID", "custom-model-123");
    }
    let model = resolve_model_id(None);
    assert_eq!(model, "custom-model-123");
    unsafe {
        std::env::remove_var("OWLY_MODEL_ID");
    }
}

#[test]
fn test_resolve_model_id_override_takes_precedence() {
    // SAFETY: We're setting env vars in a single-threaded test context
    unsafe {
        std::env::set_var("OWLY_MODEL_ID", "env-model");
    }
    let model = resolve_model_id(Some("override-model"));
    assert_eq!(model, "override-model");
    unsafe {
        std::env::remove_var("OWLY_MODEL_ID");
    }
}

#[test]
fn test_provider_needs_api_key_all_providers() {
    for provider in all_providers() {
        let _ = provider_needs_api_key(provider);
    }
}
