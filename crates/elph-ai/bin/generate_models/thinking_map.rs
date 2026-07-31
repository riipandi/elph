//! Derive full 7-key `thinkingLevelMap` for every catalog model.

use serde_json::{Value, json};

const LEVELS: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh", "max"];

/// Build a complete thinkingLevelMap (all 7 keys present).
pub fn build_thinking_level_map(
    provider_id: &str,
    model_id: &str,
    reasoning: bool,
    models_dev_model: Option<&Value>,
    previous: Option<&Value>,
) -> Value {
    if !reasoning {
        return all_null_map();
    }

    // Prefer previous explicit map when complete enough.
    if let Some(prev) = previous.and_then(|p| p.get("thinkingLevelMap"))
        && let Some(map) = complete_from_partial(prev)
    {
        return map;
    }

    // Provider-specific known maps
    if let Some(map) = provider_override_map(provider_id, model_id) {
        return map;
    }

    // models.dev reasoning_options
    if let Some(m) = models_dev_model
        && let Some(map) = from_models_dev_reasoning(m)
    {
        return map;
    }

    default_reasoning_map(provider_id)
}

fn all_null_map() -> Value {
    let mut obj = serde_json::Map::new();
    for k in LEVELS {
        obj.insert((*k).to_string(), Value::Null);
    }
    Value::Object(obj)
}

fn complete_from_partial(prev: &Value) -> Option<Value> {
    let Some(obj) = prev.as_object() else {
        return None;
    };
    let mut out = serde_json::Map::new();
    for k in LEVELS {
        out.insert((*k).to_string(), obj.get(*k).cloned().unwrap_or(Value::Null));
    }
    // Keep previous if at least one non-null wire value exists, or all nulls for non-reasoning reuse.
    Some(Value::Object(out))
}

fn map_with(pairs: &[(&str, Option<&str>)]) -> Value {
    let mut obj = serde_json::Map::new();
    for k in LEVELS {
        obj.insert((*k).to_string(), Value::Null);
    }
    for (k, v) in pairs {
        obj.insert((*k).to_string(), v.map(|s| json!(s)).unwrap_or(Value::Null));
    }
    Value::Object(obj)
}

fn provider_override_map(provider_id: &str, model_id: &str) -> Option<Value> {
    match provider_id {
        "xai" if model_id.contains("grok") || model_id.contains("build") => {
            Some(map_with(&[("low", Some("low")), ("high", Some("high")), ("max", Some("max"))]))
        }
        "anthropic" if model_id.contains("opus") || model_id.contains("sonnet-5") || model_id.contains("fable") => {
            Some(map_with(&[("xhigh", Some("xhigh")), ("max", Some("max"))]))
        }
        _ => None,
    }
}

fn from_models_dev_reasoning(m: &Value) -> Option<Value> {
    let opts = m.get("reasoning_options")?.as_array()?;
    // toggle-only → no discrete efforts; leave defaults
    let has_effort = opts.iter().any(|o| {
        o.get("type").and_then(|t| t.as_str()) == Some("effort")
            || o.get("efforts").is_some()
            || o.get("values").is_some()
    });
    if !has_effort {
        return None;
    }
    // Collect effort labels if present
    let mut efforts: Vec<String> = Vec::new();
    for o in opts {
        if let Some(arr) = o.get("efforts").or_else(|| o.get("values")).and_then(|v| v.as_array()) {
            for e in arr {
                if let Some(s) = e.as_str() {
                    efforts.push(s.to_ascii_lowercase());
                } else if let Some(s) = e.get("id").or_else(|| e.get("name")).and_then(|v| v.as_str()) {
                    efforts.push(s.to_ascii_lowercase());
                }
            }
        }
    }
    if efforts.is_empty() {
        return None;
    }
    let mut pairs = Vec::new();
    for level in ["minimal", "low", "medium", "high", "xhigh", "max"] {
        if efforts
            .iter()
            .any(|e| e == level || e.replace('-', "") == level.replace('-', ""))
        {
            pairs.push((level, Some(level)));
        }
    }
    if pairs.is_empty() {
        return None;
    }
    Some(map_with(&pairs))
}

fn default_reasoning_map(provider_id: &str) -> Value {
    match provider_id {
        "xai" => map_with(&[("low", Some("low")), ("high", Some("high")), ("max", Some("max"))]),
        "anthropic" | "amazon-bedrock" | "kimi-coding" => map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ]),
        "openai" | "openai-codex" | "azure-openai-responses" => map_with(&[
            ("minimal", Some("minimal")),
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
        ]),
        _ => map_with(&[("low", Some("low")), ("medium", Some("medium")), ("high", Some("high"))]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_reasoning_all_null() {
        let m = build_thinking_level_map("openai", "gpt-4", false, None, None);
        for k in LEVELS {
            assert!(m[k].is_null(), "{k}");
        }
    }

    #[test]
    fn xai_defaults_low_high_max() {
        let m = build_thinking_level_map("xai", "grok-4.5", true, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["high"], "high");
        assert_eq!(m["max"], "max");
        assert!(m["medium"].is_null());
    }
}
