use elph_ai::utils::error_body::{
    MAX_PROVIDER_ERROR_BODY_CHARS, NormalizedProviderError, format_provider_error, truncate_error_text,
};

#[test]
fn truncate_error_text_appends_truncation_suffix() {
    let long = "x".repeat(MAX_PROVIDER_ERROR_BODY_CHARS + 50);
    let truncated = truncate_error_text(&long, MAX_PROVIDER_ERROR_BODY_CHARS);
    assert!(truncated.contains("... [truncated 50 chars]"));
    assert!(truncated.len() < long.len());
}

#[test]
fn format_provider_error_includes_status_when_body_is_separate() {
    let norm = NormalizedProviderError {
        status: Some(403),
        body: Some(r#"{"error":"blocked by gateway WAF"}"#.to_string()),
        message: "failed".to_string(),
        message_carries_body: false,
    };
    let formatted = format_provider_error(&norm, Some("Provider error"));
    assert!(formatted.contains("403"));
    assert!(formatted.contains("blocked by gateway WAF"));
}

#[test]
fn format_provider_error_returns_message_when_body_is_embedded() {
    let norm = NormalizedProviderError {
        status: Some(500),
        body: None,
        message: r#"{"error":"upstream failed"}"#.to_string(),
        message_carries_body: true,
    };
    let formatted = format_provider_error(&norm, Some("Provider error"));
    assert_eq!(formatted, r#"Provider error (500): {"error":"upstream failed"}"#);
}

#[test]
fn format_provider_error_without_prefix_uses_status_and_body() {
    let norm = NormalizedProviderError {
        status: Some(401),
        body: Some("invalid key".to_string()),
        message: "failed".to_string(),
        message_carries_body: false,
    };
    assert_eq!(format_provider_error(&norm, None), "401: invalid key");
}
