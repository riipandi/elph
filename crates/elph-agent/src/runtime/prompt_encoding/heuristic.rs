//! Heuristics for deciding when Auto mode should encode JSON as TOON.

use std::collections::HashSet;

use serde_json::Value;

/// Returns true when `value` is a uniform array of objects (tabular JSON).
pub fn is_tabular_json(value: &Value) -> bool {
    let Value::Array(items) = value else {
        return false;
    };
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
    fn rejects_mixed_shapes() {
        let value = json!([{ "id": 1 }, { "name": "b" }]);
        assert!(!is_tabular_json(&value));
    }

    #[test]
    fn rejects_scalar_array() {
        assert!(!is_tabular_json(&json!([1, 2, 3])));
    }
}
