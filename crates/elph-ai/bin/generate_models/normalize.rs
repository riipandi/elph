//! Normalize models.dev + Elph overlays into catalog model entries.

use serde_json::{Map, Value, json};

use super::provider_sources::ProviderSource;
use super::thinking_map::build_thinking_level_map;

/// Convert a models.dev model object into an Elph catalog entry.
pub fn from_models_dev(
    provider: &ProviderSource,
    model_id: &str,
    mdev: &Value,
    previous: Option<&Value>,
    live_efforts: Option<&[String]>,
    rich: Option<&Value>,
    aimd_reasoning: Option<bool>,
    models_dev_fallback: Option<&super::models_dev::ModelsDevData>,
) -> Value {
    let name = mdev
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(model_id)
        .to_string();
    // models.dev is authoritative for the reasoning flag; ai-model-directory only
    // fills it when models.dev has no opinion (models not listed on models.dev).
    let reasoning = mdev
        .get("reasoning")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| aimd_reasoning.unwrap_or(false));
    let context = mdev
        .pointer("/limit/context")
        .and_then(|v| v.as_u64())
        .or_else(|| previous.and_then(|p| p.get("contextWindow").and_then(|v| v.as_u64())))
        .unwrap_or(128_000);
    let max_tokens = mdev
        .pointer("/limit/output")
        .and_then(|v| v.as_u64())
        .or_else(|| previous.and_then(|p| p.get("maxTokens").and_then(|v| v.as_u64())))
        .unwrap_or(context.min(64_000));
    let input = modalities_input(mdev, previous);
    let cost = merge_cost(mdev.get("cost"), previous.and_then(|p| p.get("cost")));
    let api = previous
        .and_then(|p| p.get("api").and_then(|v| v.as_str()))
        .unwrap_or(provider.default_api)
        .to_string();
    let base_url = previous
        .and_then(|p| p.get("baseUrl").and_then(|v| v.as_str()))
        .filter(|s| !s.is_empty())
        .unwrap_or(provider.default_base_url)
        .to_string();
    let thinking = build_thinking_level_map(
        provider.id,
        model_id,
        reasoning,
        Some(mdev),
        previous,
        live_efforts,
        models_dev_fallback,
    );

    let (description, knowledge_cutoff, release_date) = extract_meta(Some(mdev), rich);

    let mut entry = json!({
        "id": model_id,
        "name": name,
        "api": api,
        "provider": provider.id,
        "baseUrl": base_url,
        "reasoning": reasoning,
        "input": input,
        "contextWindow": context,
        "maxTokens": max_tokens,
        "cost": cost,
        "thinkingLevelMap": thinking,
        "description": description.unwrap_or_default(),
        "knowledgeCutoff": knowledge_cutoff.unwrap_or_default(),
        "releaseDate": release_date.unwrap_or_default(),
    });

    if let Some(prev) = previous {
        if let Some(compat) = prev.get("compat") {
            entry["compat"] = compat.clone();
        }
        if let Some(headers) = prev.get("headers") {
            entry["headers"] = headers.clone();
        }
    }
    entry
}

/// Refresh an existing Elph-only / gateway model with models.dev pricing/limits when found.
pub fn enrich_existing(
    provider: &ProviderSource,
    model_id: &str,
    previous: &Value,
    mdev: Option<&Value>,
    live_efforts: Option<&[String]>,
    rich: Option<&Value>,
    aimd_reasoning: Option<bool>,
    models_dev_fallback: Option<&super::models_dev::ModelsDevData>,
) -> Value {
    let mut entry = previous.clone();
    if !entry.is_object() {
        entry = serde_json::json!({});
    }
    let obj = entry.as_object_mut().expect("model entry object");

    obj.insert("id".into(), json!(model_id));
    obj.insert("provider".into(), json!(provider.id));

    if obj.get("api").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        obj.insert("api".into(), json!(provider.default_api));
    }
    if obj.get("baseUrl").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        obj.insert("baseUrl".into(), json!(provider.default_base_url));
    }

    // models.dev is authoritative for the reasoning flag; ai-model-directory only
    // fills it when models.dev (and the previous catalog) have no opinion.
    // For gateway-preserved IDs with no direct models.dev entry, fall back to a
    // keyword search across all providers to recover the reasoning flag from the
    // underlying family model (e.g. tencent-hy3-free → tencent/hy3).
    let mdev_reasoning = mdev.and_then(|m| m.get("reasoning").and_then(|v| v.as_bool()));
    let prev_reasoning = obj.get("reasoning").and_then(|v| v.as_bool());
    let fallback_reasoning = models_dev_fallback
        .map(|dev| {
            let kw = super::thinking_map::extract_family_keyword(model_id);
            dev.find_model_by_keyword(&kw)
                .and_then(|m| m.get("reasoning").and_then(|v| v.as_bool()))
        })
        .flatten();
    let reasoning = mdev_reasoning
        .or(fallback_reasoning)
        .or(prev_reasoning)
        .or(aimd_reasoning)
        .unwrap_or(false);
    obj.insert("reasoning".into(), json!(reasoning));

    if let Some(m) = mdev {
        if let Some(name) = m.get("name") {
            obj.insert("name".into(), name.clone());
        }
        if let Some(ctx) = m.pointer("/limit/context").and_then(|v| v.as_u64()) {
            obj.insert("contextWindow".into(), json!(ctx));
        }
        if let Some(out) = m.pointer("/limit/output").and_then(|v| v.as_u64()) {
            obj.insert("maxTokens".into(), json!(out));
        }
        let cost = merge_cost(m.get("cost"), obj.get("cost"));
        obj.insert("cost".into(), cost);
        if let Some(input) = modalities_from_mdev(m) {
            obj.insert("input".into(), input);
        }
    }

    // Ensure cost object exists
    if !obj.contains_key("cost") {
        obj.insert("cost".into(), zero_cost());
    }

    let thinking = build_thinking_level_map(
        provider.id,
        model_id,
        reasoning,
        mdev,
        Some(previous),
        live_efforts,
        models_dev_fallback,
    );
    obj.insert("thinkingLevelMap".into(), thinking);

    // Enrich with metadata-complete fields from the rich (models.json/catalog.json) index.
    let (description, knowledge_cutoff, release_date) = extract_meta(mdev, rich);
    if let Some(d) = description {
        obj.insert("description".into(), json!(d));
    }
    if let Some(k) = knowledge_cutoff {
        obj.insert("knowledgeCutoff".into(), json!(k));
    }
    if let Some(r) = release_date {
        obj.insert("releaseDate".into(), json!(r));
    }

    // Required name fallback
    if obj.get("name").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        obj.insert("name".into(), json!(model_id));
    }
    if !obj.contains_key("input") {
        obj.insert("input".into(), json!(["text"]));
    }
    if !obj.contains_key("contextWindow") {
        obj.insert("contextWindow".into(), json!(128_000));
    }
    if !obj.contains_key("maxTokens") {
        obj.insert("maxTokens".into(), json!(64_000));
    }

    entry
}

fn modalities_input(mdev: &Value, previous: Option<&Value>) -> Value {
    if let Some(v) = modalities_from_mdev(mdev) {
        return v;
    }
    previous
        .and_then(|p| p.get("input").cloned())
        .unwrap_or_else(|| json!(["text"]))
}

fn modalities_from_mdev(mdev: &Value) -> Option<Value> {
    let arr = mdev.pointer("/modalities/input")?.as_array()?;
    let mut out = Vec::new();
    for v in arr {
        if let Some(s) = v.as_str() {
            out.push(json!(s));
        }
    }
    if out.is_empty() { None } else { Some(Value::Array(out)) }
}

fn merge_cost(mdev_cost: Option<&Value>, prev_cost: Option<&Value>) -> Value {
    let mut cost = zero_cost_map();
    // Prefer models.dev when non-zero
    if let Some(c) = mdev_cost {
        apply_cost_fields(&mut cost, c);
    }
    // Fill zeros from previous
    if let Some(c) = prev_cost {
        fill_zero_from(&mut cost, c);
    }
    Value::Object(cost)
}

fn zero_cost() -> Value {
    Value::Object(zero_cost_map())
}

fn zero_cost_map() -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("input".into(), json!(0.0));
    m.insert("output".into(), json!(0.0));
    m.insert("cacheRead".into(), json!(0.0));
    m.insert("cacheWrite".into(), json!(0.0));
    m
}

fn apply_cost_fields(dest: &mut Map<String, Value>, src: &Value) {
    let Some(obj) = src.as_object() else {
        return;
    };
    for (src_key, dest_key) in [
        ("input", "input"),
        ("output", "output"),
        ("cache_read", "cacheRead"),
        ("cacheRead", "cacheRead"),
        ("cache_write", "cacheWrite"),
        ("cacheWrite", "cacheWrite"),
    ] {
        if let Some(v) = obj.get(src_key).and_then(|v| v.as_f64())
            && v > 0.0
        {
            dest.insert(dest_key.into(), json!(v));
        }
    }
}

fn fill_zero_from(dest: &mut Map<String, Value>, src: &Value) {
    let Some(obj) = src.as_object() else {
        return;
    };
    for key in ["input", "output", "cacheRead", "cacheWrite"] {
        let cur = dest.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
        if cur == 0.0
            && let Some(v) = obj.get(key).and_then(|v| v.as_f64())
            && v > 0.0
        {
            dest.insert(key.into(), json!(v));
        }
    }
}

/// Pull human-readable metadata from the merged models.dev sources.
///
/// `description` comes from the api.json model (or the rich index when missing);
/// `knowledgeCutoff` and `releaseDate` come from the rich index (`knowledge`,
/// `release_date`), which `api.json` omits.
fn extract_meta(mdev: Option<&Value>, rich: Option<&Value>) -> (Option<String>, Option<String>, Option<String>) {
    let description = mdev
        .and_then(|m| m.get("description").and_then(|v| v.as_str()))
        .or_else(|| rich.and_then(|m| m.get("description").and_then(|v| v.as_str())))
        .map(str::to_string);
    let knowledge_cutoff = rich
        .and_then(|m| m.get("knowledge").and_then(|v| v.as_str()))
        .map(str::to_string);
    let release_date = rich
        .and_then(|m| m.get("release_date").and_then(|v| v.as_str()))
        .map(str::to_string);
    (description, knowledge_cutoff, release_date)
}
