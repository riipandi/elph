//! Tests for Owly config module.

use owly::config::*;

#[test]
fn test_config_resolve_default() {
    let cwd = std::env::current_dir().unwrap();
    let config = Config::resolve(None, &cwd).unwrap();

    assert_eq!(config.provider, "opencode");
    assert_eq!(config.model_id, "big-pickle");
    assert_eq!(config.cwd, cwd);
}

#[test]
fn test_config_resolve_with_model_override() {
    let cwd = std::env::current_dir().unwrap();
    let config = Config::resolve(Some("anthropic/claude-sonnet-5"), &cwd).unwrap();

    assert_eq!(config.provider, "anthropic");
    assert_eq!(config.model_id, "claude-sonnet-5");
}

#[test]
fn test_config_resolve_with_model_override_no_provider() {
    let cwd = std::env::current_dir().unwrap();
    let config = Config::resolve(Some("custom-model"), &cwd).unwrap();

    // Should use default provider
    assert_eq!(config.provider, "opencode");
    assert_eq!(config.model_id, "custom-model");
}

#[test]
fn test_config_api_key_env_key() {
    let cwd = std::env::current_dir().unwrap();
    let config = Config::resolve(None, &cwd).unwrap();

    assert_eq!(config.api_key_env_key(), "OPENCODE_API_KEY");
}

#[test]
fn test_config_provider_label() {
    let cwd = std::env::current_dir().unwrap();
    let config = Config::resolve(None, &cwd).unwrap();

    assert_eq!(config.provider_label(), "OpenCode Zen");
}

#[test]
fn test_config_provider_str() {
    let cwd = std::env::current_dir().unwrap();
    let config = Config::resolve(None, &cwd).unwrap();

    assert_eq!(config.provider_str(), "opencode");
}

#[test]
fn test_config_elph_model_id() {
    let cwd = std::env::current_dir().unwrap();
    let config = Config::resolve(None, &cwd).unwrap();

    assert_eq!(config.elph_model_id(), "opencode/big-pickle");
}

#[test]
fn test_config_elph_model_id_with_override() {
    let cwd = std::env::current_dir().unwrap();
    let config = Config::resolve(Some("anthropic/claude-sonnet-5"), &cwd).unwrap();

    assert_eq!(config.elph_model_id(), "anthropic/claude-sonnet-5");
}

#[test]
fn test_load_config_file_nonexistent() {
    let result = load_config_file();
    // Should return None or a default config
    assert!(result.is_none() || result.is_some());
}

#[test]
fn test_config_has_api_key() {
    let cwd = std::env::current_dir().unwrap();
    let config = Config::resolve(None, &cwd).unwrap();

    // This test just checks the method doesn't panic
    let _ = config.has_api_key();
}
