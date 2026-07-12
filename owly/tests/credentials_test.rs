//! Tests for Owly credentials module.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `test/credentials.test.ts`. Original MIT License, Copyright (c) 2026 LangChain.

use owly::credentials::*;

#[test]
fn test_managed_env_keys() {
    assert!(MANAGED_ENV_KEYS.contains(&"OPENCODE_API_KEY"));
    assert!(MANAGED_ENV_KEYS.contains(&"OPENROUTER_API_KEY"));
    assert!(MANAGED_ENV_KEYS.contains(&"ANTHROPIC_API_KEY"));
    assert!(MANAGED_ENV_KEYS.contains(&"OPENAI_API_KEY"));
    assert!(MANAGED_ENV_KEYS.contains(&"GEMINI_API_KEY"));
    assert!(MANAGED_ENV_KEYS.contains(&"GOOGLE_API_KEY"));
    assert!(MANAGED_ENV_KEYS.contains(&"DEEPSEEK_API_KEY"));
    assert!(MANAGED_ENV_KEYS.contains(&"OWLY_PROVIDER"));
    assert!(MANAGED_ENV_KEYS.contains(&"OWLY_MODEL_ID"));
}

#[test]
fn test_load_env_nonexistent() {
    // Should not panic when .env file doesn't exist
    let result = load_env();
    assert!(result.is_ok());
}

#[test]
fn test_parse_env_value_unquoted() {
    // Leaves unquoted values as-is
    assert_eq!(parse_env_value("gpt-5.5"), "gpt-5.5");
    assert_eq!(parse_env_value("anthropic"), "anthropic");
}

#[test]
fn test_parse_env_value_quoted() {
    // Unquotes and unescapes double-quoted values
    assert_eq!(parse_env_value("\"https://a.example/v1\""), "https://a.example/v1");
    assert_eq!(parse_env_value("\"line1\\nline2\""), "line1\nline2");
    assert_eq!(parse_env_value("\"a\\\"b\\\\c\""), "a\"b\\c");
}

#[test]
fn test_format_env_value() {
    // Quotes and escapes values
    assert_eq!(format_env_value("abc"), "\"abc\"");
    // Input: a"b\c\nd (with newline before d)
    // Expected: "a\"b\\c\n\" (newline escaped, d included)
    assert_eq!(format_env_value("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
}

#[test]
fn test_format_env_roundtrip() {
    // Values survive a format -> parse round-trip
    let test_cases = vec![
        "simple-value",
        "value with spaces",
        "value\"with\"quotes",
        "value\\with\\backslashes",
        "value\nwith\nnewlines",
        "value\rwith\rcarriage",
        "mixed\r\nline endings",
    ];

    for value in test_cases {
        let formatted = format_env_value(value);
        let parsed = parse_env_value(&formatted);
        assert_eq!(parsed, value, "Roundtrip failed for: {}", value);
    }
}

#[test]
fn test_format_env_value_escapes_cr() {
    assert_eq!(format_env_value("a\rb"), "\"a\\rb\"");
    assert_eq!(parse_env_value("\"a\\rb\""), "a\rb");
}

#[test]
fn test_credential_diagnostics_shape() {
    let rows = get_credential_diagnostics().expect("diagnostics");
    assert!(rows.iter().any(|r| r.key == "OWLY_PROVIDER"));
    assert!(rows.iter().any(|r| r.key == "NVIDIA_API_KEY"));
}
