use std::collections::HashMap;

use elph_ai::api::bedrock_shared::{is_reserved_bedrock_header, merge_bedrock_custom_headers};

#[test]
fn injects_allowed_custom_headers() {
    let merged = merge_bedrock_custom_headers(
        &HashMap::new(),
        &HashMap::from([("x-custom".to_string(), "v".to_string())]),
    );
    assert_eq!(merged.get("x-custom").map(String::as_str), Some("v"));
}

#[test]
fn skips_reserved_headers_case_insensitively_while_applying_allowed_ones() {
    let existing = HashMap::from([
        ("authorization".to_string(), "real-auth".to_string()),
        ("x-amz-date".to_string(), "real-date".to_string()),
        ("host".to_string(), "real-host".to_string()),
    ]);
    let custom = HashMap::from([
        ("authorization".to_string(), "evil".to_string()),
        ("x-amz-date".to_string(), "evil".to_string()),
        ("x-allowed".to_string(), "ok".to_string()),
        ("Authorization".to_string(), "evil2".to_string()),
        ("X-Amz-Date".to_string(), "evil2".to_string()),
        ("HOST".to_string(), "evil3".to_string()),
    ]);
    let merged = merge_bedrock_custom_headers(&existing, &custom);
    assert_eq!(merged.get("authorization").map(String::as_str), Some("real-auth"));
    assert_eq!(merged.get("x-amz-date").map(String::as_str), Some("real-date"));
    assert_eq!(merged.get("host").map(String::as_str), Some("real-host"));
    assert_eq!(merged.get("x-allowed").map(String::as_str), Some("ok"));
    assert!(!merged.contains_key("Authorization"));
    assert!(!merged.contains_key("X-Amz-Date"));
    assert!(!merged.contains_key("HOST"));
}

#[test]
fn reserved_header_detection_matches_elph_rules() {
    assert!(is_reserved_bedrock_header("authorization"));
    assert!(is_reserved_bedrock_header("x-amz-security-token"));
    assert!(is_reserved_bedrock_header("Host"));
    assert!(!is_reserved_bedrock_header("x-custom"));
}
