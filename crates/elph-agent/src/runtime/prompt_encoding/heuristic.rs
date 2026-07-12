//! Heuristics for deciding when Auto mode should encode JSON as TOON.

use std::collections::HashSet;

use serde_json::Value;

/// Returns true when `value` is a uniform array of objects (tabular JSON).
pub fn is_tabular_json(value: &Value) -> bool {
    let Value::Array(items) = value else {
        return false;
    };
    is_uniform_object_array(items)
}

/// Locate the best tabular payload inside a JSON value.
pub fn find_tabular_payload(value: &Value) -> Option<&Value> {
    if is_tabular_json(value) {
        return Some(value);
    }

    let Value::Object(map) = value else {
        return None;
    };

    let mut tabular: Option<&Value> = None;
    for child in map.values() {
        if is_tabular_json(child) {
            if tabular.is_some() {
                return None;
            }
            tabular = Some(child);
        }
    }
    tabular
}

pub(crate) fn should_encode(value: &Value, mode: super::config::PromptEncodingMode) -> bool {
    match mode {
        super::config::PromptEncodingMode::Off => false,
        super::config::PromptEncodingMode::Toon => true,
        super::config::PromptEncodingMode::Auto => find_tabular_payload(value).is_some(),
    }
}

pub(crate) fn meets_savings_gate(json_len: usize, toon_len: usize, min_savings_ratio: f64) -> bool {
    if min_savings_ratio <= 0.0 {
        return true;
    }
    let threshold = (json_len as f64) * min_savings_ratio;
    (toon_len as f64) <= threshold
}

fn is_uniform_object_array(items: &[Value]) -> bool {
    if items.len() < 2 {
        return false;
    }

    let mut expected_keys: Option<HashSet<&str>> = None;
    for item in items {
        let Value::Object(map) = item else {
            return false;
        };
        if map.is_empty() {
            return false;
        }
        let keys: HashSet<&str> = map.keys().map(String::as_str).collect();
        match &expected_keys {
            None => expected_keys = Some(keys),
            Some(expected) if *expected == keys => {}
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tabular_array_of_uniform_objects() {
        let value = json!([
            { "id": 1, "name": "a" },
            { "id": 2, "name": "b" }
        ]);
        assert!(is_tabular_json(&value));
    }

    #[test]
    fn finds_nested_wrapper() {
        let value = json!({
            "items": [
                { "id": 1, "name": "a" },
                { "id": 2, "name": "b" }
            ]
        });
        assert!(!is_tabular_json(&value));
        assert_eq!(find_tabular_payload(&value), Some(&value["items"]));
    }

    #[test]
    fn rejects_multiple_tabular_children() {
        let value = json!({
            "a": [{ "id": 1 }, { "id": 2 }],
            "b": [{ "id": 3 }, { "id": 4 }]
        });
        assert!(find_tabular_payload(&value).is_none());
    }

    #[test]
    fn rejects_mixed_shapes() {
        let value = json!([{ "id": 1 }, { "name": "b" }]);
        assert!(!is_tabular_json(&value));
    }

    #[test]
    fn savings_gate() {
        assert!(meets_savings_gate(100, 80, 1.0));
        assert!(!meets_savings_gate(100, 120, 1.0));
        assert!(meets_savings_gate(100, 105, 1.05));
    }
}
