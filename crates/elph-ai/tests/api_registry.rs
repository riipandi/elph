use elph_ai::api::{api_for, builtin_apis};

#[test]
fn builtin_apis_registers_expected_providers() {
    let apis = builtin_apis();
    let names: Vec<_> = apis.iter().map(|(name, _)| *name).collect();
    assert!(names.contains(&"anthropic-messages"));
    assert!(names.contains(&"openai-completions"));
    assert!(names.contains(&"openai-responses"));
    assert!(names.contains(&"openai-codex-responses"));
    assert!(names.contains(&"google-generative-ai"));
    assert!(names.contains(&"bedrock-converse-stream"));
}

#[test]
fn api_for_returns_none_for_unknown_api() {
    assert!(api_for("not-a-real-api").is_none());
}

#[test]
fn api_for_returns_streaming_implementation_for_known_api() {
    assert!(api_for("anthropic-messages").is_some());
}
