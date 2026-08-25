//! Cline (usage-billing) + ClinePass live catalog builder.
//!
//! Cline's OpenAI-compatible `/v1/models` endpoint requires an API key, but the
//! public model directory endpoints do not:
//!
//! - `https://api.cline.bot/api/v1/ai/cline/recommended-models` — curated id groups
//!   (`recommended`, `free`, `clinePass`, `clineCloud`).
//! - `https://api.cline.bot/api/v1/ai/cline/models` — full detail (per-token
//!   pricing, context length, input modalities) keyed by OpenRouter-style id.
//!
//! Catalog ids:
//! - provider `cline`: the `recommended` + `free` groups (raw ids, e.g. `qwen/qwen3.8-max`).
//! - provider `cline-pass`: the `clinePass` group, prefixed (`cline-pass/kimi-k3`).
//!
//! Detail entries are paired to catalog ids by their tail segment (`kimi-k3` →
//! `moonshotai/kimi-k3`) since the recommended catalog uses display ids.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

use super::common;
use super::models_dev::ModelsDevData;
use super::term;
use super::thinking_map::build_thinking_level_map;

const RECOMMENDED_URL: &str = "https://api.cline.bot/api/v1/ai/cline/recommended-models";
const MODELS_URL: &str = "https://api.cline.bot/api/v1/ai/cline/models";

/// One built model entry ready to be written to `models/*.json`.
pub struct ClineModelEntry {
    pub id: String,
    pub json: Value,
}

/// Fetch cline + cline-pass catalogs. Returns `(provider_id, models)` for each.
pub fn fetch_cline_catalogs(models_dev: &ModelsDevData) -> BTreeMap<String, Vec<ClineModelEntry>> {
    let Some(recommended) = common::http_get_json(RECOMMENDED_URL, Duration::from_secs(30), None) else {
        term::warn("cline: recommended-models fetch failed — skipped");
        return BTreeMap::new();
    };
    let Some(detail) = common::http_get_json(MODELS_URL, Duration::from_secs(30), None) else {
        term::warn("cline: models detail fetch failed — skipped");
        return BTreeMap::new();
    };
    let Some(detail_list) = detail.get("data").and_then(|d| d.as_array()) else {
        term::warn("cline: models detail has no data array — skipped");
        return BTreeMap::new();
    };

    let mut ids_for = BTreeMap::new();
    // Cline (usage-billing): curated + free.
    let mut cline_ids: Vec<String> = Vec::new();
    for group in ["recommended", "free"] {
        if let Some(ids) = recommended.get(group).and_then(|g| g.as_array()) {
            for e in ids {
                if let Some(id) = e.get("id").and_then(|v| v.as_str()) {
                    cline_ids.push(id.to_string());
                }
            }
        }
    }
    cline_ids.sort();
    cline_ids.dedup();
    if !cline_ids.is_empty() {
        ids_for.insert("cline".to_string(), cline_ids);
    }

    // ClinePass: the clinePass group (already prefixed with `cline-pass/`).
    if let Some(ids) = recommended.get("clinePass").and_then(|v| v.as_array()) {
        let mut cp: Vec<String> = ids
            .iter()
            .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        cp.sort();
        cp.dedup();
        if !cp.is_empty() {
            ids_for.insert("cline-pass".to_string(), cp);
        }
    }

    let mut out = BTreeMap::new();
    for (provider, ids) in ids_for {
        let mut entries = Vec::new();
        for id in &ids {
            let detail_entry = find_detail(detail_list, id);
            let entry = build_entry(&provider, id, detail_entry, models_dev);
            entries.push(ClineModelEntry {
                id: id.clone(),
                json: entry,
            });
        }
        term::live_pricing(&provider, entries.len());
        out.insert(provider, entries);
    }
    out
}

/// Pair a catalog id to its full detail entry by tail segment match.
fn find_detail<'a>(list: &'a [Value], catalog_id: &str) -> Option<&'a Value> {
    let tail = catalog_id
        .split('/')
        .next_back()
        .unwrap_or(catalog_id)
        .to_ascii_lowercase();
    for e in list {
        let did = e.get("id").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
        let detail_tail = did.split('/').next_back().unwrap_or(&did);
        if detail_tail == tail {
            return Some(e);
        }
    }
    None
}

fn build_entry(provider: &str, id: &str, detail: Option<&Value>, models_dev: &ModelsDevData) -> Value {
    let (cost, context, input) = match detail {
        Some(d) => (pricing_of(d), context_of(d), modalities_of(d)),
        None => (default_cost(), 128_000, vec!["text".to_string()]),
    };
    let name = detail
        .and_then(|d| d.get("name").and_then(|v| v.as_str()))
        .unwrap_or(id)
        .to_string();
    // Resolve the 7-key thinking map from models.dev (authoritative reasoning_options)
    // with the provider-family override as fallback. The catalog id tail matches the
    // underlying family model (e.g. `anthropic/claude-opus-5` → `claude-opus-5`).
    let mdev = super::models_dev::find_model_fuzzy(models_dev, id).map(|(_, m)| m);
    let thinking = build_thinking_level_map(provider, id, true, mdev.as_ref(), None, None, Some(models_dev));
    serde_json::json!({
        "id": id,
        "name": name,
        "api": "openai-completions",
        "provider": provider,
        "baseUrl": "https://api.cline.bot/api/v1",
        "reasoning": true,
        "input": input,
        "contextWindow": context,
        "maxTokens": context.min(128_000),
        "cost": cost,
        "thinkingLevelMap": thinking,
        "description": detail.and_then(|d| d.get("description").and_then(|v| v.as_str())).unwrap_or_default(),
    })
}

fn pricing_of(detail: &Value) -> Value {
    let p = detail.get("pricing");
    let i = f64_of(p.and_then(|v| v.get("prompt"))) * 1_000_000.0;
    let o = f64_of(p.and_then(|v| v.get("completion"))) * 1_000_000.0;
    let cr = f64_of(p.and_then(|v| v.get("input_cache_read"))) * 1_000_000.0;
    serde_json::json!({ "input": i, "output": o, "cacheRead": cr, "cacheWrite": 0.0 })
}

fn f64_of(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn context_of(detail: &Value) -> u64 {
    detail
        .get("context_length")
        .and_then(|v| v.as_u64())
        .or_else(|| detail.get("context").and_then(|v| v.as_u64()))
        .unwrap_or(128_000)
}

fn modalities_of(detail: &Value) -> Vec<String> {
    // Only keep modalities the Elph schema supports: text, image, pdf, video, audio.
    // Cline's API also reports `file`, which Elph does not handle yet.
    const SUPPORTED: &[&str] = &["text", "image", "pdf", "video", "audio"];
    detail
        .pointer("/architecture/input_modalities")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| SUPPORTED.contains(s))
                .map(str::to_string)
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| vec!["text".to_string()])
}

fn default_cost() -> Value {
    serde_json::json!({ "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0 })
}
