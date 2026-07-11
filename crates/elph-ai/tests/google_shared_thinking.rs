use elph_ai::api::google_shared::{is_thinking_part, retain_thought_signature};
use serde_json::json;

#[test]
fn detects_thinking_parts_by_thought_flag() {
    assert!(is_thinking_part(&json!({ "text": "reasoning", "thought": true })));
    assert!(!is_thinking_part(&json!({ "text": "answer" })));
}

#[test]
fn retain_thought_signature_prefers_incoming_non_empty_value() {
    assert_eq!(retain_thought_signature(Some("old"), Some("new")), Some("new".to_string()));
    assert_eq!(retain_thought_signature(Some("old"), Some("")), Some("old".to_string()));
    assert_eq!(retain_thought_signature(None, Some("new")), Some("new".to_string()));
}
