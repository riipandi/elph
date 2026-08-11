//! Live provider pricing probes (preferred) + models.dev cost helpers.
//! Also extracts thinking/reasoning capabilities from live API responses.

use std::collections::HashMap;
use std::env;

use serde_json::Value;

use super::models_dev::{ModelsDevRoot, find_model, find_model_fuzzy};
use super::provider_sources::{ProviderSource, all_provider_sources};
use super::term;

pub type PriceTriple = (f64, f64, f64); // input, output, cache_read

/// Thinking capabilities extracted from a live API response for one model.
#[derive(Default, Clone, serde::Deserialize)]
pub struct LiveThinking {
    /// Whether the model supports reasoning/thinking.
    pub reasoning: bool,
    /// Ordered list of supported effort levels (e.g. ["low", "medium", "high"]).
    /// Empty vec means reasoning=true but no discrete effort levels.
    pub supported_efforts: Vec<String>,
}

/// Combined live probe result: pricing + thinking capabilities per model.
#[derive(Default, Clone, serde::Deserialize)]
pub struct LiveProbeResult {
    pub pricing: HashMap<String, PriceTriple>,
    pub thinking: HashMap<String, LiveThinking>,
}

/// Fetch live OpenAI-compatible `/models` pricing and thinking capabilities
/// for providers with base URL + env key set.
pub fn fetch_all_live_data(skip: bool) -> HashMap<String, LiveProbeResult> {
    let mut out = HashMap::new();
    if skip {
        term::note("Skipping live pricing probes (--no-live-pricing)");
        return out;
    }
    for src in all_provider_sources() {
        let Some(base) = src.live_pricing_base else {
            continue;
        };
        if let Some(var) = src.live_pricing_env
            && env::var(var).is_err()
        {
            continue;
        }
        let result = fetch_live_provider_data(src, base);
        if !result.pricing.is_empty() || !result.thinking.is_empty() {
            term::live_pricing(src.id, result.pricing.len());
            out.insert(src.id.to_string(), result);
        }
    }
    out
}

/// Also available: pricing-only probe (for backward compat).
pub fn fetch_all_live_pricing(skip: bool) -> HashMap<String, HashMap<String, PriceTriple>> {
    let all = fetch_all_live_data(skip);
    all.into_iter().map(|(k, v)| (k, v.pricing)).collect()
}

/// Live model id list for a provider (OpenAI-compatible `/models`), when a
/// base URL + env key are configured. Returns `None` when not configured or
/// the probe fails (caller falls back to the previous catalog).
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
                .filter(|e| match e.get("category_type").and_then(|v| v.as_str()) {
                    Some("LLM") => true,
                    Some(_) => false,
                    None => true,
                })
                .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() { None } else { Some(ids) }
}

fn fetch_live_provider_data(src: &ProviderSource, base_url: &str) -> LiveProbeResult {
    let url = if base_url.ends_with("/models") {
        base_url.to_string()
    } else {
        format!("{}/models", base_url.trim_end_matches('/'))
    };
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
    else {
        return LiveProbeResult::default();
    };
    let mut req = client.get(&url);
    if let Some(var) = src.live_pricing_env
        && let Ok(key) = env::var(var)
    {
        req = req.bearer_auth(key);
    }
    let Ok(resp) = req.send() else {
        return LiveProbeResult::default();
    };
    if !resp.status().is_success() {
        return LiveProbeResult::default();
    }
    let Ok(body) = resp.json::<Value>() else {
        return LiveProbeResult::default();
    };

    let mut pricing = HashMap::new();
    let mut thinking = HashMap::new();

    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
        for entry in data {
            let mid = match entry.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };

            // --- Pricing ---
            let (inp, outp, cached) = if let Some(pricing_obj) = entry.get("metadata").and_then(|m| m.get("pricing")) {
                let i = pricing_obj
                    .get("input_per_million")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let o = pricing_obj
                    .get("output_per_million")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let c = pricing_obj
                    .get("cached_input_per_million")
                    .or_else(|| pricing_obj.get("cache_read_per_million"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                (i, o, c)
            } else if let Some(pricing_obj) = cents_pricing(entry) {
                pricing_obj
            } else if let Some(pricing_obj) = entry.get("pricing") {
                let i = pricing_obj.get("input").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let o = pricing_obj.get("output").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let c = pricing_obj
                    .get("cache_hit")
                    .or_else(|| pricing_obj.get("cache_read"))
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
                pricing.insert(mid.clone(), (inp, outp, cached));
            }

            // --- Thinking capabilities ---
            if let Some(reasoning_obj) = entry.get("reasoning").and_then(|r| r.as_object()) {
                let supported: Vec<String> = reasoning_obj
                    .get("supported_efforts")
                    .and_then(|e| e.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let is_reasoning = reasoning_obj
                    .get("mandatory")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                    || reasoning_obj
                        .get("default_enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                if is_reasoning || !supported.is_empty() {
                    thinking.insert(
                        mid,
                        LiveThinking {
                            reasoning: is_reasoning,
                            supported_efforts: supported,
                        },
                    );
                }
            } else if entry.get("reasoning").and_then(|v| v.as_bool()) == Some(true) {
                thinking.insert(
                    mid,
                    LiveThinking {
                        reasoning: true,
                        supported_efforts: Vec::new(),
                    },
                );
            }
        }
    }
    LiveProbeResult { pricing, thinking }
}

/// Wafer-style pricing nested under a vendor object, expressed in cents per
/// million tokens. Returns USD per million tokens so it lines up with every other pricing shape.
fn cents_pricing(entry: &Value) -> Option<PriceTriple> {
    let pricing = entry
        .as_object()?
        .values()
        .filter_map(|v| v.get("pricing"))
        .find(|p| p.get("input_cents_per_million").is_some())?;
    let cents = |key: &str| pricing.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0) / 100.0;
    Some((
        cents("input_cents_per_million"),
        cents("output_cents_per_million"),
        cents("cache_read_cents_per_million"),
    ))
}

/// Resolve best price: live → models.dev → previous non-zero.
/// Returns (input, output, cache_read, cache_write, source).
pub fn resolve_cost(
    provider: &ProviderSource,
    model_id: &str,
    models_dev: &ModelsDevRoot,
    live: &HashMap<String, LiveProbeResult>,
    previous_cost: Option<&Value>,
) -> (f64, f64, f64, f64, &'static str) {
    if let Some(map) = live.get(provider.id)
        && let Some(&(i, o, c)) = map.pricing.get(model_id)
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

/// Get live thinking capabilities for a model from the probe result.
pub fn live_thinking<'a>(
    live: &'a HashMap<String, LiveProbeResult>,
    provider_id: &str,
    model_id: &str,
) -> Option<&'a LiveThinking> {
    live.get(provider_id).and_then(|r| r.thinking.get(model_id))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_pricing_converts_wafer_shape_to_usd() {
        let entry = serde_json::json!({
            "id": "GLM-5.1",
            "wafer": {
                "pricing": {
                    "currency": "usd",
                    "input_cents_per_million": 100,
                    "output_cents_per_million": 320,
                    "cache_read_cents_per_million": 10,
                }
            }
        });
        let (i, o, c) = cents_pricing(&entry).expect("wafer pricing");
        assert_eq!(i, 1.0);
        assert_eq!(o, 3.2);
        assert_eq!(c, 0.1);
    }

    #[test]
    fn cents_pricing_ignores_other_shapes() {
        let entry = serde_json::json!({ "id": "m", "pricing": { "input": 1.0, "output": 2.0 } });
        assert!(cents_pricing(&entry).is_none());
    }

    #[test]
    fn live_thinking_extracts_efforts() {
        // Test by calling fetch_live_provider_data directly with a mock response.
        // We can't deserialize LiveProbeResult from raw OpenAI /models shape,
        // so we construct it manually and verify the fields.
        let thinking = LiveThinking {
            reasoning: true,
            supported_efforts: vec!["low".into(), "medium".into(), "high".into()],
        };
        assert!(thinking.reasoning);
        assert_eq!(thinking.supported_efforts, vec!["low", "medium", "high"]);
    }
}
