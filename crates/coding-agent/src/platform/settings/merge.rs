//! Deep-merge for settings JSON: objects recurse, arrays and scalars replace.

use serde_json::Value;

/// Deep-merge `overlay` into `base` (objects recurse; arrays and other JSON types replace).
pub fn deep_merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(key) {
                    Some(base_value) => deep_merge(base_value, overlay_value),
                    None => {
                        base_map.insert(key.clone(), overlay_value.clone());
                    }
                }
            }
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_merge_replaces_arrays_and_scalars() {
        let mut base = serde_json::json!({
            "models": { "enabled": ["a/b"], "defaultThinkingLevel": "high" },
            "ui": { "showThinking": true }
        });
        let overlay = serde_json::json!({
            "models": { "enabled": ["x/y", "z/w"] },
            "ui": { "showThinking": false }
        });
        deep_merge(&mut base, &overlay);
        assert_eq!(base["ui"]["showThinking"], false);
        assert_eq!(base["models"]["enabled"], serde_json::json!(["x/y", "z/w"]));
        assert_eq!(base["models"]["defaultThinkingLevel"], "high");
    }
}
