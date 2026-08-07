//! Live provider pricing probes (preferred) + models.dev cost helpers.

use std::collections::HashMap;
use std::env;

use serde_json::Value;

use super::models_dev::{ModelsDevRoot, find_model, find_model_fuzzy};
use super::provider_sources::{ProviderSource, all_provider_sources};
use super::term;

pub type PriceTriple = (f64, f64, f64); // input, output, cache_read

/// Fetch live OpenAI-compatible `/models` pricing for providers with base URL + env key set.
pub fn fetch_all_live_pricing(skip: bool) -> HashMap<String, HashMap<String, PriceTriple>> {
    let mut out = HashMap::new();
    if skip {
        term::note("Skipping live pricing probes (--no-live-pricing)");
        return out;
    }
    for src in all_provider_sources() {
        let Some(base) = src.live_pricing_base else {
            continue;
        };
        // Only probe when key is available (avoids noisy 401s).
        if let Some(var) = src.live_pricing_env
            && env::var(var).is_err()
        {
            continue;
        }
        let prices = fetch_live_provider_pricing(src, base);
        if !prices.is_empty() {
            term::live_pricing(src.id, prices.len());
            out.insert(src.id.to_string(), prices);
        }
    }
    out
}

/// Live model id list for a provider (OpenAI-compatible `/models`), when a
/// base URL + env key are configured. Returns `None` when not configured or
/// the probe fails (caller falls back to the previous catalog).
///
/// When the API exposes a `category_type` field (e.g. Infron/OneRouter), only
/// `LLM` entries are kept so image/video models never pollute the chat catalog.
pub fn fetch_live_model_ids(src: &ProviderSource) -> Option<Vec<String>> {
    let base = src.live_pricing_base?;
    if let Some(var) = src.live_pricing_env
        && env::var(var).is_err()
    {
        return None;
    }
    let url = if base.ends_with("/models") {
        base.to_string()
    } else {
        format!("{}/models", base.trim_end_matches('/'))
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok()?;
    let mut req = client.get(&url);
    if let Some(var) = src.live_pricing_env
        && let Ok(key) = env::var(var)
    {
        req = req.bearer_auth(key);
    }
    let resp = req.send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: Value = resp.json().ok()?;
    let ids: Vec<String> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|e| {
                    // Keep LLM chat models only when the API categorizes them.
                    match e.get("category_type").and_then(|v| v.as_str()) {
                        Some("LLM") => true,
                        Some(_) => false,
                        None => true, // no category field → keep all
                    }
                })
                .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() { None } else { Some(ids) }
}

fn fetch_live_provider_pricing(src: &ProviderSource, base_url: &str) -> HashMap<String, PriceTriple> {
    let url = if base_url.ends_with("/models") {
        base_url.to_string()
    } else {
        format!("{}/models", base_url.trim_end_matches('/'))
    };
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
    else {
        return HashMap::new();
    };
    let mut req = client.get(&url);
    if let Some(var) = src.live_pricing_env
        && let Ok(key) = env::var(var)
    {
        req = req.bearer_auth(key);
    }
    let Ok(resp) = req.send() else {
        return HashMap::new();
    };
    if !resp.status().is_success() {
        return HashMap::new();
    }
    let Ok(body) = resp.json::<Value>() else {
        return HashMap::new();
    };

    let mut out = HashMap::new();
    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
        for entry in data {
            let mid = match entry.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            // Try several pricing shapes across providers:
            // 1. models.dev style: metadata.pricing.{input_per_million,...}
            // 2. Hyper style:      pricing.{input,output,cache_hit,cache_create}
            // 3. Infron style:     min_prompt_price / min_completion_price (per 1M)
            let (inp, outp, cached) = if let Some(pricing) = entry.get("metadata").and_then(|m| m.get("pricing")) {
                let i = pricing.get("input_per_million").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let o = pricing
                    .get("output_per_million")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let c = pricing
                    .get("cached_input_per_million")
                    .or_else(|| pricing.get("cache_read_per_million"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                (i, o, c)
            } else if let Some(pricing) = entry.get("pricing") {
                let i = pricing.get("input").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let o = pricing.get("output").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let c = pricing
                    .get("cache_hit")
                    .or_else(|| pricing.get("cache_read"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                (i, o, c)
            } else {
                let i = entry.get("min_prompt_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let o = entry
                    .get("min_completion_price")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                (i, o, 0.0)
            };
            if inp > 0.0 || outp > 0.0 {
                out.insert(mid, (inp, outp, cached));
            }
        }
    }
    out
}

/// Resolve best price: live → models.dev → previous non-zero.
/// Returns (input, output, cache_read, cache_write, source).
pub fn resolve_cost(
    provider: &ProviderSource,
    model_id: &str,
    models_dev: &ModelsDevRoot,
    live: &HashMap<String, HashMap<String, PriceTriple>>,
    previous_cost: Option<&Value>,
) -> (f64, f64, f64, f64, &'static str) {
    if let Some(map) = live.get(provider.id)
        && let Some(&(i, o, c)) = map.get(model_id)
        && (i > 0.0 || o > 0.0)
    {
        let cw = previous_cost
            .and_then(|p| p.get("cacheWrite").and_then(|v| v.as_f64()))
            .unwrap_or(0.0);
        return (i, o, c, cw, "live-api");
    }

    if let Some(m) = find_model(models_dev, provider.models_dev_keys, model_id)
        && let Some((i, o, cr, cw)) = cost_from_mdev(m)
    {
        return (i, o, cr, cw, "models.dev");
    }

    if let Some((_, m)) = find_model_fuzzy(models_dev, model_id)
        && let Some((i, o, cr, cw)) = cost_from_mdev(&m)
    {
        return (i, o, cr, cw, "models.dev");
    }

    if let Some(c) = previous_cost {
        let i = c.get("input").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let o = c.get("output").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let cr = c.get("cacheRead").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let cw = c.get("cacheWrite").and_then(|v| v.as_f64()).unwrap_or(0.0);
        return (i, o, cr, cw, "previous");
    }
    (0.0, 0.0, 0.0, 0.0, "none")
}

fn cost_from_mdev(m: &Value) -> Option<(f64, f64, f64, f64)> {
    let c = m.get("cost")?;
    let i = c.get("input").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let o = c.get("output").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let cr = c
        .get("cache_read")
        .or_else(|| c.get("cacheRead"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let cw = c
        .get("cache_write")
        .or_else(|| c.get("cacheWrite"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if i > 0.0 || o > 0.0 { Some((i, o, cr, cw)) } else { None }
}

/// Apply resolved cost onto a model entry JSON (prefer non-zero).
pub fn apply_cost(entry: &mut Value, i: f64, o: f64, cr: f64, cw: f64) {
    let Some(obj) = entry.as_object_mut() else {
        return;
    };
    let cost = obj
        .entry("cost".to_string())
        .or_insert_with(|| serde_json::json!({ "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0 }));
    if let Value::Object(c) = cost {
        set_if_positive(c, "input", i);
        set_if_positive(c, "output", o);
        set_if_positive(c, "cacheRead", cr);
        set_if_positive(c, "cacheWrite", cw);
        for k in ["input", "output", "cacheRead", "cacheWrite"] {
            c.entry(k.to_string()).or_insert(serde_json::json!(0.0));
        }
    }
}

fn set_if_positive(c: &mut serde_json::Map<String, Value>, key: &str, new: f64) {
    if new > 0.0 {
        c.insert(key.into(), serde_json::json!(new));
    }
}
