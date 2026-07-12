//! Diagnostics and error handling for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/diagnostics.ts`. Original MIT License, Copyright (c) 2026 LangChain.
//!
//! Provides redaction of secrets from error messages and diagnostic output.

use std::sync::LazyLock;

/// API key environment variables that should be redacted
static API_KEY_ENV_VARS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "OPENCODE_API_KEY",
        "OPENROUTER_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GOOGLE_API_KEY",
        "DEEPSEEK_API_KEY",
        "GROQ_API_KEY",
        "FIREWORKS_API_KEY",
        "TOGETHER_API_KEY",
        "MISTRAL_API_KEY",
        "NVIDIA_API_KEY",
        "LANGSMITH_API_KEY",
    ]
});

/// Redacts secrets from text before it is shown to the user or written to a log.
///
/// This is a security boundary: any error message, header value, or provider
/// response body that could contain a credential must pass through here first.
/// It removes:
/// 1. The exact values of secrets currently set in the environment
/// 2. Anything matching known key/token shapes (OpenAI/OpenRouter `sk-...`,
///    `Bearer ...`, LangSmith `ls...`, and "Incorrect API key provided: ..." phrasing)
pub fn sanitize_diagnostic_text(value: &str) -> String {
    let mut sanitized = value.to_string();

    // Redact exact values of environment variables
    for key in API_KEY_ENV_VARS.iter() {
        if let Ok(secret) = std::env::var(key)
            && !secret.is_empty()
        {
            sanitized = sanitized.replace(&secret, &format!("[REDACTED:{key}]"));
        }
    }

    // Use regex for all pattern replacements
    // "Incorrect API key provided: ..." phrasing
    let re_incorrect_key = regex::Regex::new(r"(?i)Incorrect API key provided:\s*([^\s.]+)").unwrap();
    let sanitized = re_incorrect_key.replace_all(&sanitized, "Incorrect API key provided: [REDACTED:API_KEY]");

    // Bearer tokens
    let re_bearer = regex::Regex::new(r"Bearer\s+[A-Za-z0-9._~+/=-]+").unwrap();
    let sanitized = re_bearer.replace_all(&sanitized, "Bearer [REDACTED]");

    // OpenRouter sk-or-v1- tokens
    let re_sk_or = regex::Regex::new(r"sk-or-v1-[A-Za-z0-9_-]+").unwrap();
    let sanitized = re_sk_or.replace_all(&sanitized, "[REDACTED:OPENROUTER_API_KEY]");

    // OpenAI sk- tokens
    let re_sk = regex::Regex::new(r"sk-[A-Za-z0-9_-]+").unwrap();
    let sanitized = re_sk.replace_all(&sanitized, "[REDACTED:API_KEY]");

    // LangSmith ls_/lsv_ keys
    let re_ls = regex::Regex::new(r"ls[v_][A-Za-z0-9_-]+").unwrap();
    let sanitized = re_ls.replace_all(&sanitized, "[REDACTED:LANGSMITH_API_KEY]");

    sanitized.to_string()
}

/// Recognizes an OpenRouter/provider 500 response so a friendlier, actionable
/// message can be shown instead of a raw stack trace.
pub fn is_provider_server_error(error: &anyhow::Error, message: &str) -> bool {
    let error_str = format!("{error}");
    error_str.contains("500") || message.contains("Internal Server Error")
}

/// Produces a user-facing error message: a friendly note for provider 500s,
/// otherwise the error's own message with any secrets redacted.
pub fn get_error_message(error: &anyhow::Error) -> String {
    let message = format!("{error}");

    if is_provider_server_error(error, &message) {
        "Provider returned 500 Internal Server Error. Try retrying or switching models with --model. Run with OWLY_DEBUG=1 to show provider metadata.".to_string()
    } else {
        sanitize_diagnostic_text(&message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_diagnostic_text() {
        // Test that the function doesn't panic
        let result = sanitize_diagnostic_text("test message");
        assert_eq!(result, "test message");
    }

    #[test]
    fn test_sanitize_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOi.J9.abc-123";
        let result = sanitize_diagnostic_text(input);
        assert!(!result.contains("eyJhbGciOi.J9.abc-123"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_sk_tokens() {
        let input = "token sk-abcDEF123_456 rejected";
        let result = sanitize_diagnostic_text(input);
        assert!(!result.contains("sk-abcDEF123_456"));
        assert!(result.contains("[REDACTED:API_KEY]"));
    }

    #[test]
    fn test_sanitize_openrouter_tokens() {
        let input = "using sk-or-v1-deadbeef00 now";
        let result = sanitize_diagnostic_text(input);
        assert!(!result.contains("sk-or-v1-deadbeef00"));
        assert!(result.contains("[REDACTED:OPENROUTER_API_KEY]"));
    }

    #[test]
    fn test_sanitize_langsmith_keys() {
        let input = "langsmith lsv_1234abcd tracing";
        let result = sanitize_diagnostic_text(input);
        assert!(!result.contains("lsv_1234abcd"));
        assert!(result.contains("[REDACTED:LANGSMITH_API_KEY]"));
    }

    #[test]
    fn test_sanitize_incorrect_api_key() {
        let input = "Incorrect API key provided: myLeakedKey. Check your account.";
        let result = sanitize_diagnostic_text(input);
        assert!(!result.contains("myLeakedKey"));
        assert!(result.contains("[REDACTED:API_KEY]"));
    }

    #[test]
    fn test_get_error_message_provider_500() {
        let error = anyhow::anyhow!("500 Internal Server Error");
        let message = get_error_message(&error);
        assert!(message.contains("500 Internal Server Error"));
        assert!(message.contains("--model"));
    }

    #[test]
    fn test_is_provider_server_error_500() {
        let error = anyhow::anyhow!("500 Internal Server Error");
        let message = format!("{error}");
        assert!(is_provider_server_error(&error, &message));
    }

    #[test]
    fn test_is_provider_server_error_non_500() {
        let error = anyhow::anyhow!("400 Bad Request");
        let message = format!("{error}");
        assert!(!is_provider_server_error(&error, &message));
    }

    #[test]
    fn test_sanitize_openrouter_response_body() {
        // Test that the function doesn't panic with JSON input
        let body = r#"{"model":"glm-5.2"}"#;
        let result = sanitize_diagnostic_text(body);
        assert!(result.contains("glm-5.2"));
    }

    #[test]
    fn test_sanitize_openrouter_response_body_with_bearer() {
        // Test that Bearer tokens in JSON are redacted
        let body = r#"{"authorization":"Bearer abc123"}"#;
        let result = sanitize_diagnostic_text(body);
        assert!(!result.contains("Bearer abc123"));
        assert!(result.contains("Bearer [REDACTED]"));
    }
}
