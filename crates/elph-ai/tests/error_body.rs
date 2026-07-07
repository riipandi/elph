use elph_ai::utils::error_body::{
    MAX_PROVIDER_ERROR_BODY_CHARS, NormalizedProviderError, ProviderSdkError, ThrownValue, format_provider_error,
    normalize_provider_error, truncate_error_text,
};
use serde_json::json;

fn sdk_error(error: ProviderSdkError) -> anyhow::Error {
    anyhow::Error::new(error)
}

#[test]
fn truncate_error_text_appends_truncation_suffix() {
    let long = "x".repeat(MAX_PROVIDER_ERROR_BODY_CHARS + 50);
    let truncated = truncate_error_text(&long, MAX_PROVIDER_ERROR_BODY_CHARS);
    assert!(truncated.contains("... [truncated 50 chars]"));
    assert!(truncated.len() < long.len());
}

#[test]
fn extracts_status_and_body_from_mistral_shaped_error() {
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: "Mistral request failed".to_string(),
        status_code: Some(403),
        status: None,
        body: Some(r#"{"error":"blocked by gateway WAF"}"#.to_string()),
        parsed_error: None,
        bedrock_metadata_http_status: None,
        bedrock_response_status_code: None,
        bedrock_response_body: None,
    }));
    assert_eq!(norm.status, Some(403));
    assert_eq!(norm.body.as_deref(), Some(r#"{"error":"blocked by gateway WAF"}"#));
    assert!(!norm.message_carries_body);
}

#[test]
fn reads_parsed_body_from_openai_api_error_when_message_is_opaque() {
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: "403 status code (no body)".to_string(),
        status_code: None,
        status: Some(403),
        body: None,
        parsed_error: Some(json!({ "error": "blocked by gateway WAF" })),
        bedrock_metadata_http_status: None,
        bedrock_response_status_code: None,
        bedrock_response_body: None,
    }));
    assert_eq!(norm.status, Some(403));
    assert_eq!(norm.body.as_deref(), Some(r#"{"error":"blocked by gateway WAF"}"#));
    assert!(!norm.message_carries_body);
}

#[test]
fn preserves_message_when_google_genai_folds_body_into_it() {
    let body = json!({ "error": { "code": 403, "message": "Permission denied" } });
    let message = serde_json::to_string(&body).expect("json");
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: message.clone(),
        status_code: None,
        status: Some(403),
        body: None,
        parsed_error: None,
        bedrock_metadata_http_status: None,
        bedrock_response_status_code: None,
        bedrock_response_body: None,
    }));
    assert_eq!(norm.status, Some(403));
    assert!(norm.message_carries_body);
    assert_eq!(norm.message, message);
}

#[test]
fn extracts_status_and_body_from_bedrock_shaped_service_exception() {
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: "UnknownError".to_string(),
        status_code: None,
        status: None,
        body: None,
        parsed_error: None,
        bedrock_metadata_http_status: Some(403),
        bedrock_response_status_code: Some(403),
        bedrock_response_body: Some(r#"{"message":"blocked by gateway WAF"}"#.to_string()),
    }));
    assert_eq!(norm.status, Some(403));
    assert_eq!(norm.body.as_deref(), Some(r#"{"message":"blocked by gateway WAF"}"#));
    assert!(!norm.message_carries_body);
}

#[test]
fn json_stringifies_non_error_thrown_value() {
    let norm = normalize_provider_error(&anyhow::Error::new(ThrownValue(json!({ "reason": "boom" }))));
    assert_eq!(norm.status, None);
    assert_eq!(norm.body, None);
    assert_eq!(norm.message, r#"{"reason":"boom"}"#);
    assert!(!norm.message_carries_body);
}

#[test]
fn treats_empty_parsed_body_object_as_no_body() {
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: "403 status code (no body)".to_string(),
        status_code: None,
        status: Some(403),
        body: None,
        parsed_error: Some(json!({})),
        bedrock_metadata_http_status: None,
        bedrock_response_status_code: None,
        bedrock_response_body: None,
    }));
    assert_eq!(norm.body, None);
    assert!(norm.message_carries_body);
}

#[test]
fn truncates_body_at_cap() {
    let long_body = "x".repeat(MAX_PROVIDER_ERROR_BODY_CHARS + 50);
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: "failed".to_string(),
        status_code: Some(500),
        status: None,
        body: Some(long_body.clone()),
        parsed_error: None,
        bedrock_metadata_http_status: None,
        bedrock_response_status_code: None,
        bedrock_response_body: None,
    }));
    assert!(norm.body.as_ref().expect("body").contains("... [truncated 50 chars]"));
    assert!(norm.body.as_ref().expect("body").len() < long_body.len());
}

#[test]
fn sets_message_carries_body_when_message_contains_extracted_body() {
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: "500: upstream exploded".to_string(),
        status_code: Some(500),
        status: None,
        body: Some("upstream exploded".to_string()),
        parsed_error: None,
        bedrock_metadata_http_status: None,
        bedrock_response_status_code: None,
        bedrock_response_body: None,
    }));
    assert!(norm.message_carries_body);
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

#[test]
fn format_provider_error_surfaces_openai_api_error_without_prefix() {
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: "403 status code (no body)".to_string(),
        status_code: None,
        status: Some(403),
        body: None,
        parsed_error: Some(json!({ "error": "blocked by gateway WAF" })),
        bedrock_metadata_http_status: None,
        bedrock_response_status_code: None,
        bedrock_response_body: None,
    }));
    let formatted = format_provider_error(&norm, None);
    assert!(formatted.contains("403"));
    assert!(formatted.contains("blocked by gateway WAF"));
    assert_ne!(formatted, "403 status code (no body)");
}

#[test]
fn format_provider_error_applies_provider_prefix_with_status_and_body() {
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: "403 status code (no body)".to_string(),
        status_code: None,
        status: Some(403),
        body: None,
        parsed_error: Some(json!({ "error": "blocked by gateway WAF" })),
        bedrock_metadata_http_status: None,
        bedrock_response_status_code: None,
        bedrock_response_body: None,
    }));
    assert_eq!(
        format_provider_error(&norm, Some("OpenAI API error")),
        r#"OpenAI API error (403): {"error":"blocked by gateway WAF"}"#
    );
}

#[test]
fn format_provider_error_preserves_message_when_it_carries_body() {
    let body = json!({ "error": { "message": "Permission denied" } });
    let message = serde_json::to_string(&body).expect("json");
    let norm = normalize_provider_error(&sdk_error(ProviderSdkError {
        message: message.clone(),
        status_code: None,
        status: Some(403),
        body: None,
        parsed_error: None,
        bedrock_metadata_http_status: None,
        bedrock_response_status_code: None,
        bedrock_response_body: None,
    }));
    assert_eq!(
        format_provider_error(&norm, Some("OpenAI API error")),
        format!("OpenAI API error (403): {message}")
    );
}

#[test]
fn format_provider_error_returns_bare_message_for_non_error_value() {
    let norm = normalize_provider_error(&anyhow::Error::new(ThrownValue(json!({ "reason": "boom" }))));
    assert_eq!(format_provider_error(&norm, None), r#"{"reason":"boom"}"#);
}
