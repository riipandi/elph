mod common;

use common::error_assistant_message;
use elph_ai::utils::retry::is_retryable;

const OPENAI_EXPLICIT_RETRY: &str = "An error occurred while processing your request. You can retry your request, or contact us through our help center at help.openai.com if the error persists. Please include the request ID req_******** in your message.";
const BEDROCK_EXPLICIT_RETRY: &str =
    r#"{"message":"The system encountered an unexpected error during processing. Try your request again."}"#;

#[test]
fn matches_explicit_provider_retry_guidance() {
    assert!(is_retryable(&error_assistant_message(OPENAI_EXPLICIT_RETRY)));
    assert!(is_retryable(&error_assistant_message(BEDROCK_EXPLICIT_RETRY)));
}

#[test]
fn keeps_provider_limit_errors_non_retryable() {
    assert!(!is_retryable(&error_assistant_message("429 quota exceeded")));
}

#[test]
fn classifies_transient_assistant_errors() {
    assert!(is_retryable(&error_assistant_message("overloaded_error")));
    assert!(is_retryable(&error_assistant_message("524 status code (no body)")));
}

#[test]
fn treats_rate_limit_errors_as_retryable() {
    assert!(is_retryable(&error_assistant_message("429 rate limit exceeded")));
}

#[test]
fn treats_service_unavailable_as_retryable() {
    assert!(is_retryable(&error_assistant_message("503 service unavailable")));
}

#[test]
fn treats_billing_errors_as_non_retryable() {
    assert!(!is_retryable(&error_assistant_message("billing hard limit reached")));
}
