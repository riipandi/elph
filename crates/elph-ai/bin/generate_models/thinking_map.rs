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

fn is_valid_effort(s: &str) -> bool {
    LEVELS.contains(&s)
}

/// Build a complete thinkingLevelMap (all 7 keys present).
pub fn build_thinking_level_map(
    provider_id: &str,
    model_id: &str,
    reasoning: bool,
    models_dev_model: Option<&Value>,
    previous: Option<&Value>,
    live_reasoning_efforts: Option<&[String]>,
    models_dev_fallback: Option<&super::models_dev::ModelsDevData>,
) -> Value {
    // 1. Live API supported_efforts (gateway providers like OpenRouter) — strongest signal.
    //    Checked before the `!reasoning` guard so an OpenRouter `supported_efforts` array
    //    still wins even when models.dev lists the model as non-reasoning.
    if let Some(efforts) = live_reasoning_efforts
        && let Some(map) = map_from_efforts(efforts)
    {
        return map;
    }

    if !reasoning {
        // 1b. Gateway-preserved IDs may have stale `reasoning: false` from a previous
        //     catalog while the underlying family model IS reasoning-capable.
        //     Search models.dev by keyword to recover the correct thinking levels.
        if let Some(dev) = models_dev_fallback {
            let kw = extract_family_keyword(model_id);
            if let Some(mdev_fallback) = dev.find_model_by_keyword(&kw)
                && mdev_fallback.get("reasoning").and_then(|v| v.as_bool()) == Some(true)
            {
                // Found a reasoning-capable family member — apply provider override
                // based on the matched family instead of silently returning all-null.
                if let Some(map) = provider_override_map(provider_id, model_id) {
                    return map;
                }
            }
        }
        return all_null_map();
    }

    // 2. Direct models.dev reasoning_options (authoritative per-model data).
    if let Some(m) = models_dev_model
        && let Some(map) = from_models_dev_reasoning(m)
    {
        return map;
    }

    // 3. Provider-family override — used as fallback when no direct models.dev
    //    data exists for this exact model id.
    if let Some(map) = provider_override_map(provider_id, model_id) {
        return map;
    }

    // 4. Preserve previous explicit map when it has at least one non-null value.
    // This protects intentional Elph overlays from being overwritten.
    // Values are normalized so stale uppercase entries (e.g. "HIGH", "MINIMAL")
    // from prior generations are corrected in place.
    if let Some(prev) = previous.and_then(|p| p.get("thinkingLevelMap"))
        && let Some(obj) = prev.as_object()
        && obj.values().any(|v| !v.is_null())
    {
        let mut out = serde_json::Map::new();
        for k in LEVELS {
            let v = obj.get(*k);
            out.insert(
                (*k).to_string(),
                match v {
                    Some(Value::String(s)) => {
                        let norm = normalize_effort_label(s);
                        if is_valid_effort(&norm) {
                            json!(norm)
                        } else {
                            Value::Null
                        }
                    }
                    Some(_) => Value::Null,
                    None => Value::Null,
                },
            );
        }
        return Value::Object(out);
    }

    // 5. No source found.
    all_null_map()
}

/// Extract a family keyword from a model id for models.dev fallback lookup.
/// Picks the most distinctive token (ignoring vendor prefixes, numeric suffixes,
/// and common gateway modifiers like `-free`, `-preview`).
pub(crate) fn extract_family_keyword(model_id: &str) -> String {
    let base = model_id.split('/').next_back().unwrap_or(model_id);
    // Normalize: lowercase, replace colons with hyphens
    let normalized = base.to_ascii_lowercase().replace(':', "-");
    // Split on separators
    let parts: Vec<&str> = normalized
        .split(['-', '/'])
        .filter(|s| !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit()))
        .collect();

    // Known vendor/organization prefixes that are too broad for matching
    const VENDOR_PREFIXES: &[&str] = &[
        "tencent",
        "google",
        "deepinfra",
        "openrouter",
        "kilo",
        "huggingface",
        "nvidia",
        "bytedance",
        "zi",
        "zai",
        "moonshotai",
        "xiaomi",
        "alibaba",
        "meta",
        "amazon",
        "apple",
        "microsoft",
        "amazon",
    ];

    // Score each part: prefer tokens with digits or model-specific markers
    let scored: Vec<(&str, i32)> = parts
        .iter()
        .map(|p| {
            let has_digit = p.chars().any(|c| c.is_ascii_digit());
            let is_vendor = VENDOR_PREFIXES.contains(p);
            // Model family names get extra weight; vendor prefixes get penalized
            let is_family_name = matches!(
                *p,
                "claude"
                    | "sonnet"
                    | "opus"
                    | "fable"
                    | "haiku"
                    | "gemma"
                    | "gemini"
                    | "qwen"
                    | "kimi"
                    | "deepseek"
                    | "glm"
                    | "nemotron"
                    | "ling"
                    | "seed"
                    | "mimo"
                    | "muse"
                    | "spark"
                    | "kat"
                    | "inkling"
                    | "command"
                    | "cogito"
                    | "aya"
                    | "solar"
                    | "granite"
                    | "llama"
                    | "mistral"
                    | "mixtral"
                    | "doubao"
                    | "nano"
            );
            let score = if is_family_name {
                100
            } else if has_digit {
                10
            } else {
                1
            } - if is_vendor { 8 } else { 0 };
            (*p, score)
        })
        .collect();

    // Pick the highest-scoring part; tie-break by length
    scored
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.len().cmp(&b.0.len())))
        .map(|(s, _)| s.to_string())
        .unwrap_or_else(|| normalized.clone())
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
    let base_id = model_id.split('/').next_back().unwrap_or(model_id);
    let base_id_lower = base_id.to_ascii_lowercase();
    let provider_lower = provider_id.to_ascii_lowercase();
    // Origin-provider overrides first (exact provider + base id pattern).
    let origin = match provider_lower.as_str() {
        "xai" if base_id_lower.contains("grok") || base_id_lower.contains("build") => {
            Some(map_with(&[("low", Some("low")), ("high", Some("high")), ("max", Some("max"))]))
        }
        "anthropic"
            if base_id_lower.contains("opus")
                || base_id_lower.contains("sonnet-5")
                || base_id_lower.contains("fable")
                || base_id_lower.contains("sonnet-4-6")
                || base_id_lower.contains("opus-4-6")
                || base_id_lower.contains("opus-4.6")
                || base_id_lower.contains("opus-4.7")
                || base_id_lower.contains("opus-4.8")
                || base_id_lower.contains("opus-5")
                || base_id_lower.contains("fable-5") =>
        {
            Some(map_with(&[("xhigh", Some("xhigh")), ("max", Some("max"))]))
        }
        "anthropic" if base_id_lower.contains("haiku-4-5") || base_id_lower.contains("haiku-4.5") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ])),
        "anthropic"
            if model_id.to_ascii_lowercase().contains("sonnet-4-5")
                || model_id.to_ascii_lowercase().contains("sonnet-4.5")
                || base_id_lower.contains("opus-4-5")
                || base_id_lower.contains("opus-4.5")
                || base_id_lower.contains("opus-4.1")
                || base_id_lower.contains("opus-4 ")
                || base_id_lower.contains("opus-4") =>
        {
            Some(map_with(&[
                ("low", Some("low")),
                ("medium", Some("medium")),
                ("high", Some("high")),
                ("max", Some("max")),
            ]))
        }
        "openai" | "openrouter" | "hyper" | "kilo" | "infron" | "tokenrouter"
            if is_openai_reasoning_model(&base_id_lower) =>
        {
            Some(map_with(&[
                ("off", Some("off")),
                ("low", Some("low")),
                ("medium", Some("medium")),
                ("high", Some("high")),
                ("xhigh", Some("xhigh")),
            ]))
        }
        "openai" | "openrouter" | "hyper" | "kilo" | "infron" | "tokenrouter"
            if base_id_lower.starts_with('o') && !base_id_lower.starts_with("oh") =>
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
    match &base_id_lower {
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
        // Gemma 4 — low / high / max
        b if b.contains("gemma") => {
            Some(map_with(&[("low", Some("low")), ("high", Some("high")), ("max", Some("max"))]))
        }
        // Qwen3 / Qwen3.x — off / low / medium / high / xhigh
        b if b.contains("qwen3") || b.contains("qwen-3") => Some(map_with(&[
            ("off", Some("off")),
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("xhigh", Some("xhigh")),
        ])),
        // Kimi K2.x — off / low / medium / high / xhigh
        b if b.contains("kimi") && b.contains("k2") => Some(map_with(&[
            ("off", Some("off")),
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("xhigh", Some("xhigh")),
        ])),
        // MiniMax M2.x / M3 — low / medium / high / max
        b if b.contains("minimax") && (b.contains("m2") || b.contains("m3")) => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ])),
        // GLM 4.x+ / GLM-5 — off / low / medium / high / xhigh
        b if (b.contains("glm") && (b.contains("4.") || b.contains("4-") || b.contains("5")))
            || b.contains("glm-5") =>
        {
            Some(map_with(&[
                ("off", Some("off")),
                ("low", Some("low")),
                ("medium", Some("medium")),
                ("high", Some("high")),
                ("xhigh", Some("xhigh")),
            ]))
        }
        // Nemotron — low / high
        b if b.contains("nemotron") => Some(map_with(&[("low", Some("low")), ("high", Some("high"))])),
        // Cohere Command — low / medium / high (match "command", not "cohere" which is in org prefix)
        b if b.contains("command") && !b.contains("embed") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
        ])),
        // Inkling — low / high
        b if b.contains("inkling") => Some(map_with(&[("low", Some("low")), ("high", Some("high"))])),
        // Claude 3.x Sonnet — low / medium / high / max (adaptive thinking)
        b if b.contains("claude") && b.contains("sonnet") && b.contains("3") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ])),
        // Claude 3.5 Sonnet — low / medium / high / max (adaptive thinking)
        b if b.contains("claude") && b.contains("3.5") && b.contains("sonnet") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ])),
        // DeepSeek R1 — off / low / medium / high / xhigh
        b if b.contains("deepseek") && b.contains("r1") => Some(map_with(&[
            ("off", Some("off")),
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("xhigh", Some("xhigh")),
        ])),
        // DeepSeek v3/v4 — low / high / max (per api-docs.deepseek.com thinking_mode)
        // reasoning_effort accepts: low/high/max; default effort is high
        b if b.contains("deepseek") => {
            Some(map_with(&[("low", Some("low")), ("high", Some("high")), ("max", Some("max"))]))
        }
        // ByteDance Seed — low / medium / high / max
        b if b.contains("seed") && !b.contains("nano") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ])),
        // Cohere Aya — low / medium / high
        b if b.contains("aya") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
        ])),
        // Qwen (non-Qwen3) — off / low / medium / high / xhigh
        b if b.contains("qwen") && !b.contains("qwen3") && !b.contains("qwen-3") => Some(map_with(&[
            ("off", Some("off")),
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("xhigh", Some("xhigh")),
        ])),
        // Upstage Solar — low / medium / high / max
        b if b.contains("solar") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ])),
        // Ling (AntLing) — low / medium / high
        b if b.contains("ling") && !b.contains("claude") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
        ])),
        // KAT Coder — low / medium / high
        b if b.contains("kat") && b.contains("coder") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
        ])),
        // Muse Spark — low / medium / high
        b if b.contains("muse") && b.contains("spark") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
        ])),
        // MiMo — low / medium / high
        b if b.contains("mimo") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
        ])),
        // Tencent Hy3 — low / medium / high / max (confirmed reasoning by models.dev)
        b if b.contains("hy3") || b.contains("tencent") && b.contains("hy") => Some(map_with(&[
            ("low", Some("low")),
            ("medium", Some("medium")),
            ("high", Some("high")),
            ("max", Some("max")),
        ])),
        // Gemini flash/preview non-batch — low / medium / high
        b if b.contains("gemini") && !b.contains("batch") && !b.contains(":free") && !b.contains("-batch") => {
            Some(map_with(&[
                ("low", Some("low")),
                ("medium", Some("medium")),
                ("high", Some("high")),
            ]))
        }
        // Gemini batch/free routes — no discrete thinking effort levels
        b if b.contains("gemini") && (b.contains(":batch") || b.contains(":free") || b.contains("-batch")) => {
            Some(all_null_map())
        }
        // Gateway batch/free routes for any family — pre-computed pricing, no thinking effort API
        b if b.contains(":batch") || b.contains(":free") || b.contains("-batch") => Some(all_null_map()),
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
        let m = build_thinking_level_map("openai", "gpt-4", false, None, None, None, None);
        for k in LEVELS {
            assert!(m[k].is_null(), "{k}");
        }
    }

    #[test]
    fn xai_defaults_low_high_max() {
        let m = build_thinking_level_map("xai", "grok-4.5", true, None, None, None, None);
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
        let m = build_thinking_level_map("openai", "gpt-5.4", true, None, None, Some(&efforts), None);
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
        let m = build_thinking_level_map("openai", "gpt-5.2-pro", true, Some(&mdev), None, None, None);
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
        let m = build_thinking_level_map("unknown-provider", "some-model", true, None, Some(&prev), None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["max"], "max");
    }

    #[test]
    fn uppercase_values_in_previous_map_are_normalized() {
        // Stale uppercase values from prior generations must be lowercased.
        // Use a provider without a family override so step 4 (preserve overlay) applies.
        let prev = json!({
            "thinkingLevelMap": {
                "off": null, "minimal": null, "low": null, "medium": null,
                "high": "HIGH", "xhigh": null, "max": null
            }
        });
        let m = build_thinking_level_map("unknown-provider", "some-model", true, None, Some(&prev), None, None);
        assert_eq!(m["high"], "high");
        assert!(m["low"].is_null());
    }

    #[test]
    fn invalid_effort_value_in_previous_map_becomes_null() {
        // Non-canonical values that don't normalize to a known level are dropped.
        // Use a provider without a family override so step 4 (preserve overlay) applies.
        let prev = json!({
            "thinkingLevelMap": {
                "off": null, "minimal": "UNKNOWN_LEVEL", "low": null, "medium": null,
                "high": null, "xhigh": null, "max": null
            }
        });
        let m = build_thinking_level_map("unknown-provider", "some-model", true, None, Some(&prev), None, None);
        // "UNKNOWN_LEVEL" normalizes to "unknownlevel" which is not a valid effort → null
        assert!(m["minimal"].is_null());
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
        let m = build_thinking_level_map("openai", "gpt-5.4", true, None, Some(&prev), None, None);
        // Provider override gives off/low/medium/high/xhigh (no max)
        assert_eq!(m["off"], "off");
        assert_eq!(m["low"], "low");
        assert!(m["max"].is_null());
    }

    #[test]
    fn gemma_family_low_high_max() {
        let m = build_thinking_level_map("hyper", "gemma-4-26b-a4b-it", true, None, None, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["high"], "high");
        assert_eq!(m["max"], "max");
        assert!(m["medium"].is_null());
        assert!(m["xhigh"].is_null());
    }

    #[test]
    fn qwen3_family_off_to_xhigh() {
        let m = build_thinking_level_map("hyper", "qwen3.7-max", true, None, None, None, None);
        assert_eq!(m["off"], "off");
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["xhigh"], "xhigh");
        assert!(m["minimal"].is_null());
        assert!(m["max"].is_null());
    }

    #[test]
    fn kimi_k2_family_off_to_xhigh() {
        let m = build_thinking_level_map("moonshotai", "kimi-k2.7-code", true, None, None, None, None);
        assert_eq!(m["off"], "off");
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["xhigh"], "xhigh");
        assert!(m["minimal"].is_null());
        assert!(m["max"].is_null());
    }

    #[test]
    fn minimax_m2_m3_family_low_to_max() {
        let m = build_thinking_level_map("together", "MiniMaxAI/MiniMax-M2.7", true, None, None, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["max"], "max");
        assert!(m["off"].is_null());
        assert!(m["xhigh"].is_null());
    }

    #[test]
    fn glm_45_5_family_off_to_xhigh() {
        let m = build_thinking_level_map("huggingface", "zai-org/GLM-4.5", true, None, None, None, None);
        assert_eq!(m["off"], "off");
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["xhigh"], "xhigh");
        assert!(m["minimal"].is_null());
        assert!(m["max"].is_null());
    }

    #[test]
    fn nemotron_family_low_high() {
        let m = build_thinking_level_map(
            "together",
            "nvidia/nemotron-3.5-lightning-30b-a3b",
            true,
            None,
            None,
            None,
            None,
        );
        assert_eq!(m["low"], "low");
        assert_eq!(m["high"], "high");
        assert!(m["medium"].is_null());
        assert!(m["xhigh"].is_null());
    }

    #[test]
    fn cohere_command_family_low_to_high() {
        let m = build_thinking_level_map(
            "huggingface",
            "CohereLabs/command-a-reasoning-08-2025",
            true,
            None,
            None,
            None,
            None,
        );
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert!(m["off"].is_null());
        assert!(m["xhigh"].is_null());
    }

    #[test]
    fn inkling_family_low_high() {
        let m =
            build_thinking_level_map("fireworks", "accounts/fireworks/models/inkling", true, None, None, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["high"], "high");
        assert!(m["medium"].is_null());
        assert!(m["xhigh"].is_null());
    }

    #[test]
    fn gemini_batch_free_returns_all_null() {
        // Batch/free routes have no discrete thinking effort — all-null is correct
        let m = build_thinking_level_map("openrouter", "google/gemini-2.5-flash:batch", true, None, None, None, None);
        for k in ["off", "minimal", "low", "medium", "high", "xhigh", "max"] {
            assert!(m[k].is_null(), "{k} should be null for batch route");
        }
    }

    #[test]
    fn openrouter_free_route_returns_all_null() {
        // Gateway free routes (e.g. :free suffix) have no discrete thinking effort API
        let m = build_thinking_level_map(
            "openrouter",
            "dots-studio/dots-3-note-preview:free",
            true,
            None,
            None,
            None,
            None,
        );
        for k in ["off", "minimal", "low", "medium", "high", "xhigh", "max"] {
            assert!(m[k].is_null(), "{k} should be null for free route");
        }
    }

    #[test]
    fn claude_35_sonnet_low_medium_high_max() {
        let m = build_thinking_level_map("cloudflare_ai_gateway", "claude-3.5-sonnet", true, None, None, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["max"], "max");
        assert!(m["off"].is_null());
    }

    #[test]
    fn deepseek_r1_distill_off_to_xhigh() {
        let m = build_thinking_level_map(
            "huggingface",
            "deepseek-ai/DeepSeek-R1-Distill-Llama-70B",
            true,
            None,
            None,
            None,
            None,
        );
        assert_eq!(m["off"], "off");
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["xhigh"], "xhigh");
        assert!(m["minimal"].is_null());
        assert!(m["max"].is_null());
    }

    #[test]
    fn deepseek_v4_low_high_max() {
        // Per https://api-docs.deepseek.com/guides/thinking_mode/
        // reasoning_effort accepts: low/high/max
        let m = build_thinking_level_map("deepseek", "deepseek-v4-flash", true, None, None, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["high"], "high");
        assert_eq!(m["max"], "max");
        assert!(m["medium"].is_null());
        assert!(m["xhigh"].is_null());
        assert!(m["minimal"].is_null());
        assert!(m["off"].is_null());
    }

    #[test]
    fn seed_family_low_to_max() {
        let m = build_thinking_level_map("openrouter", "bytedance-seed/seed-2-1-turbo", true, None, None, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["max"], "max");
        assert!(m["off"].is_null());
    }

    #[test]
    fn gemini_non_batch_low_medium_high() {
        // Non-batch Gemini flash/preview should get low/medium/high, not all-null
        let m = build_thinking_level_map("infron", "google/gemini-2.5-flash-image", true, None, None, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert!(m["xhigh"].is_null());
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

    #[test]
    fn tencent_hy3_low_to_max() {
        // Gateway-preserved IDs (e.g. tencent-hy3-free) should match the underlying family
        let m = build_thinking_level_map("nara-router", "tencent-hy3-free", true, None, None, None, None);
        assert_eq!(m["low"], "low");
        assert_eq!(m["medium"], "medium");
        assert_eq!(m["high"], "high");
        assert_eq!(m["max"], "max");
        assert!(m["off"].is_null());
    }
}
