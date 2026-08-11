//! Derive full 7-key `thinkingLevelMap` for every catalog model.
//!
//! Source precedence (first match wins):
//! 1. Live API `reasoning.supported_efforts` (gateway providers like OpenRouter) — strongest signal,
//!    checked even before the `reasoning` boolean guard.
//! 2. models.dev `reasoning_options` (direct provider catalogs)
//! 3. Provider-family override map (known defaults from official docs)
//! 4. Previous complete map (preserved Elph overlay)
//! 5. Unresolved — all values null, never silently guessed

use serde_json::{Value, json};

const LEVELS: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh", "max"];

/// Build a complete thinkingLevelMap (all 7 keys present).
pub fn build_thinking_level_map(
    provider_id: &str,
    model_id: &str,
    reasoning: bool,
    models_dev_model: Option<&Value>,
    previous: Option<&Value>,
    live_reasoning_efforts: Option<&[String]>,
) -> Value {
    // 1. Live API supported_efforts (gateway providers like OpenRouter) — strongest signal.
    //    Checked before the `!reasoning` guard so an OpenRouter `supported_efforts` array
    //    still wins even when models.dev lists the model as non-reasoning.
    if let Some(efforts) = live_reasoning_efforts {
        if let Some(map) = map_from_efforts(efforts) {
            return map;
        }
    }

    if !reasoning {
        return all_null_map();
    }

    // 2. models.dev reasoning_options.
    if let Some(m) = models_dev_model {
        if let Some(map) = from_models_dev_reasoning(m) {
            return map;
        }
    }

    // 3. Provider-specific known maps (authoritative from official docs).
    if let Some(map) = provider_override_map(provider_id, model_id) {
        return map;
    }

    // 4. Preserve previous explicit map when it has at least one non-null value.
    // This protects intentional Elph overlays from being overwritten.
    if let Some(prev) = previous.and_then(|p| p.get("thinkingLevelMap"))
        && let Some(obj) = prev.as_object()
        && obj.values().any(|v| !v.is_null())
    {
        let mut out = serde_json::Map::new();
        for k in LEVELS {
            out.insert((*k).to_string(), obj.get(*k).cloned().unwrap_or(Value::Null));
        }
        return Value::Object(out);
    }

    // 5. No source found.
    all_null_map()
}

fn all_null_map() -> Value {
    let mut obj = serde_json::Map::new();
    for k in LEVELS {
        obj.insert((*k).to_string(), Value::Null);
    }
    Value::Object(obj)
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

/// Map a list of effort strings (from live API or models.dev) onto the 7-key schema.
/// Handles aliases: "none" → "off", "min" → "minimal".
fn map_from_efforts(efforts: &[String]) -> Option<Value> {
    if efforts.is_empty() {
        return None;
    }
    let normalized: Vec<String> = efforts.iter().map(|e| normalize_effort_label(e)).collect();
    let mut pairs = Vec::new();
    for level in LEVELS {
        if normalized.iter().any(|e| e == *level) {
            pairs.push((*level, Some(*level)));
        }
    }
    if pairs.is_empty() {
        return None;
    }
    Some(map_with(&pairs))
}

/// Normalize an effort label to one of the 7 canonical keys.
fn normalize_effort_label(s: &str) -> String {
    let lower = s.to_ascii_lowercase().replace('-', "");
    match lower.as_str() {
        "none" | "off" => "off".to_string(),
        "min" | "minimal" => "minimal".to_string(),
        "low" => "low".to_string(),
        "medium" | "med" => "medium".to_string(),
        "high" => "high".to_string(),
        "xhigh" | "very-high" | "veryhigh" => "xhigh".to_string(),
        "max" => "max".to_string(),
        "default" => "medium".to_string(), // model's default effort = medium
        _ => lower,
    }
}

/// Provider-family override maps based on official documentation.
fn provider_override_map(provider_id: &str, model_id: &str) -> Option<Value> {
    // For gateway providers, extract the base model id after the last slash
    let base_id = model_id.split('/').last().unwrap_or(model_id);
    // Origin-provider overrides first (exact provider + base id pattern).
    let origin = match provider_id {
        "xai" if base_id.contains("grok") || base_id.contains("build") => {
            Some(map_with(&[("low", Some("low")), ("high", Some("high")), ("max", Some("max"))]))
        }
        "anthropic"
            if base_id.contains("opus")
                || base_id.contains("sonnet-5")
                || base_id.contains("fable")
                || base_id.contains("sonnet-4-6")
                || base_id.contains("opus-4-6")
                || base_id.contains("opus-4.6")
                || base_id.contains("opus-4.7")
                || base_id.contains("opus-4.8")
                || base_id.contains("opus-5")
                || base_id.contains("fable-5") =>
        {
            Some(map_with(&[("xhigh", Some("xhigh")), ("max", Some("max"))]))
        }
        "anthropic" if base_id.contains("haiku-4-5") || base_id.contains("haiku-4.5") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ])),
        "anthropic"
            if model_id.contains("sonnet-4-5")
                || model_id.contains("sonnet-4.5")
                || base_id.contains("opus-4-5")
                || base_id.contains("opus-4.5")
                || base_id.contains("opus-4.1")
                || base_id.contains("opus-4 ")
                || base_id.contains("opus-4") =>
        {
            Some(map_with(&[
                ("low", Some("low")),
                ("medium", Some("medium")),
                ("high", Some("high")),
                ("max", Some("max")),
            ]))
        }
        "openai" | "openrouter" | "hyper" | "kilo" | "infron" | "tokenrouter" if is_openai_reasoning_model(base_id) => {
            Some(map_with(&[
                ("off", Some("off")),
                ("low", Some("low")),
                ("medium", Some("medium")),
                ("high", Some("high")),
                ("xhigh", Some("xhigh")),
            ]))
        }
        "openai" | "openrouter" | "hyper" | "kilo" | "infron" | "tokenrouter"
            if base_id.starts_with('o') && !base_id.starts_with("oh") =>
        {
            Some(map_with(&[
                ("low", Some("low")),
                ("medium", Some("medium")),
                ("high", Some("high")),
            ]))
        }
        _ => None,
    };
    if origin.is_some() {
        return origin;
    }
    // Gateway providers (nara-router, neuralwatt, tokenrouter, …) re-host the same
    // underlying models. Fall back to a base-id family match so well-known reasoning
    // families still receive an accurate map even when hosted on a non-origin gateway.
    match base_id {
        b if b.contains("grok") || b.contains("build") => {
            Some(map_with(&[("low", Some("low")), ("high", Some("high")), ("max", Some("max"))]))
        }
        b if b.contains("opus")
            || b.contains("sonnet-5")
            || b.contains("fable")
            || b.contains("sonnet-4-6")
            || b.contains("opus-4-6")
            || b.contains("opus-4.6")
            || b.contains("opus-4.7")
            || b.contains("opus-4.8")
            || b.contains("opus-5")
            || b.contains("fable-5") =>
        {
            Some(map_with(&[("xhigh", Some("xhigh")), ("max", Some("max"))]))
        }
        b if b.contains("haiku-4-5") || b.contains("haiku-4.5") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ])),
        b if b.contains("sonnet-4-5")
            || b.contains("sonnet-4.5")
            || b.contains("opus-4-5")
            || b.contains("opus-4.5")
            || b.contains("opus-4.1")
            || b.contains("opus-4 ")
            || b.contains("opus-4") =>
        {
            Some(map_with(&[
                ("low", Some("low")),
                ("medium", Some("medium")),
                ("high", Some("high")),
                ("max", Some("max")),
            ]))
        }
        b if is_openai_reasoning_model(b) => Some(map_with(&[
            ("off", Some("off")),
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("xhigh", Some("xhigh")),
        ])),
        _ => None,
    }
}

/// Check if an OpenAI model ID belongs to a reasoning-capable model family.
fn is_openai_reasoning_model(model_id: &str) -> bool {
    let lower = model_id.to_ascii_lowercase();
    // GPT-5.x series with reasoning support
    lower.contains("gpt-5")
        // O-series reasoning models
        || lower.starts_with("o1-")
        || lower.starts_with("o3-")
        // ChatGPT custom models
        || lower.contains("chatgpt") && lower.contains("gpt")
}

fn from_models_dev_reasoning(m: &Value) -> Option<Value> {
    let opts = m.get("reasoning_options")?.as_array()?;
    // Collect effort labels from all effort-type options
    let mut efforts: Vec<String> = Vec::new();
    for o in opts {
        let typ = o.get("type").and_then(|t| t.as_str());
        if typ != Some("effort") {
            continue;
        }
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
    map_from_efforts(&efforts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_reasoning_all_null() {
        let m = build_thinking_level_map("openai", "gpt-4", false, None, None, None);
        for k in LEVELS {
            assert!(m[k].is_null(), "{k}");
        }
    }

    #[test]
    fn xai_defaults_low_high_max() {
        let m = build_thinking_level_map("xai", "grok-4.5", true, None, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["high"], "high");
        assert_eq!(m["max"], "max");
        assert!(m["medium"].is_null());
    }

    #[test]
    fn live_api_efforts_map_correctly() {
        let efforts = vec![
            "none".to_string(),
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
            "xhigh".to_string(),
        ];
        let m = build_thinking_level_map("openai", "gpt-5.4", true, None, None, Some(&efforts));
        assert_eq!(m["off"], "off");
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["xhigh"], "xhigh");
        assert!(m["minimal"].is_null());
        assert!(m["max"].is_null());
    }

    #[test]
    fn models_dev_effort_values() {
        let mdev = json!({
            "reasoning": true,
            "reasoning_options": [{
                "type": "effort",
                "values": ["medium", "high", "xhigh"]
            }]
        });
        let m = build_thinking_level_map("openai", "gpt-5.2-pro", true, Some(&mdev), None, None);
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["xhigh"], "xhigh");
        assert!(m["off"].is_null());
    }

    #[test]
    fn previous_map_preserved_for_unknown_provider() {
        // When no provider override exists, previous map should be preserved
        let prev = json!({
            "thinkingLevelMap": {
                "off": null, "minimal": null, "low": "low", "medium": "medium",
                "high": "high", "xhigh": null, "max": "max"
            }
        });
        let m = build_thinking_level_map("unknown-provider", "some-model", true, None, Some(&prev), None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["max"], "max");
    }

    #[test]
    fn provider_override_wins_over_previous() {
        // When a provider override exists, it takes precedence over previous
        let prev = json!({
            "thinkingLevelMap": {
                "off": null, "minimal": null, "low": "low", "medium": "medium",
                "high": "high", "xhigh": null, "max": "max"
            }
        });
        let m = build_thinking_level_map("openai", "gpt-5.4", true, None, Some(&prev), None);
        // Provider override gives off/low/medium/high/xhigh (no max)
        assert_eq!(m["off"], "off");
        assert_eq!(m["low"], "low");
        assert!(m["max"].is_null());
    }

    #[test]
    fn openai_gpt5_reasoning_model_detection() {
        assert!(is_openai_reasoning_model("gpt-5.4"));
        assert!(is_openai_reasoning_model("gpt-5.4-mini"));
        assert!(is_openai_reasoning_model("gpt-5.5-pro"));
        assert!(is_openai_reasoning_model("gpt-5"));
        assert!(!is_openai_reasoning_model("gpt-4o"));
        assert!(!is_openai_reasoning_model("gpt-3.5-turbo"));
        assert!(is_openai_reasoning_model("o3-mini"));
        assert!(is_openai_reasoning_model("o1-preview"));
    }

    #[test]
    fn normalize_effort_label_maps_aliases() {
        assert_eq!(super::normalize_effort_label("none"), "off");
        assert_eq!(super::normalize_effort_label("None"), "off");
        assert_eq!(super::normalize_effort_label("min"), "minimal");
        assert_eq!(super::normalize_effort_label("minimal"), "minimal");
        assert_eq!(super::normalize_effort_label("xhigh"), "xhigh");
        assert_eq!(super::normalize_effort_label("very-high"), "xhigh");
        assert_eq!(super::normalize_effort_label("medium"), "medium");
        assert_eq!(super::normalize_effort_label("med"), "medium");
    }
}
