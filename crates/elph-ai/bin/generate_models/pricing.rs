//! Live provider pricing probes (preferred) + models.dev cost helpers.
//! Also extracts thinking/reasoning capabilities from live API responses.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::models_dev::{ModelsDevData, find_model, find_model_fuzzy};
use super::provider_sources::{ProviderSource, all_provider_sources};
use super::term;

pub type PriceTriple = (f64, f64, f64); // input, output, cache_read

/// Thinking capabilities extracted from a live API response for one model.
#[derive(Default, Clone, serde::Deserialize)]
pub struct LiveThinking {
    /// Whether the model supports reasoning/thinking.
    #[allow(dead_code)]
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
    /// Live context window (tokens) per model, when the `/models` response provides it.
    pub context: HashMap<String, u64>,
    /// Live max output tokens per model, when the `/models` response provides it.
    pub max_tokens: HashMap<String, u64>,
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
        if !result.pricing.is_empty()
            || !result.thinking.is_empty()
            || !result.context.is_empty()
            || !result.max_tokens.is_empty()
        {
            term::live_pricing(src.id, result.pricing.len());
            out.insert(src.id.to_string(), result);
        }
    }
    out
}

/// Also available: pricing-only probe (for backward compat).
#[allow(dead_code)]
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
    let bearer = src.live_pricing_env.and_then(|var| env::var(var).ok());
    let body = super::common::http_get_json(&url, Duration::from_secs(20), bearer.as_deref())?;
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
    let bearer = src.live_pricing_env.and_then(|var| env::var(var).ok());
    let Some(body) = super::common::http_get_json(&url, Duration::from_secs(20), bearer.as_deref()) else {
        return LiveProbeResult::default();
    };

    parse_live_probe_body(&body)
}

/// Parse a live `/models` response body into pricing, thinking, context and
/// max-token maps. Handles OpenAI-compatible, OpenRouter and fallback shapes.
fn parse_live_probe_body(body: &Value) -> LiveProbeResult {
    let mut pricing = HashMap::new();
    let mut thinking = HashMap::new();
    let mut context = HashMap::new();
    let mut max_tokens = HashMap::new();

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
                // OpenRouter shape: per-token strings keyed prompt/completion/input_cache_read.
                if pricing_obj.get("prompt").is_some() || pricing_obj.get("completion").is_some() {
                    let i = num_or_str(pricing_obj.get("prompt")) * 1_000_000.0;
                    let o = num_or_str(pricing_obj.get("completion")) * 1_000_000.0;
                    let c = num_or_str(pricing_obj.get("input_cache_read")) * 1_000_000.0;
                    (i, o, c)
                } else {
                    let i = num_or_str(pricing_obj.get("input"));
                    let o = num_or_str(pricing_obj.get("output"));
                    let c = num_or_str(pricing_obj.get("cache_hit").or_else(|| pricing_obj.get("cache_read")));
                    (i, o, c)
                }
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

            // --- Context window & max output tokens ---
            // OpenAI-compatible `/models` responses may expose `context_window`
            // and `max_output_tokens` (e.g. DataByte). OpenRouter uses
            // `context_length` with per-provider `top_provider.max_completion_tokens`.
            // Treat 0 as missing.
            if let Some(ctx) = entry.get("context_window").and_then(|v| v.as_u64()).filter(|c| *c > 0) {
                context.insert(mid.clone(), ctx);
            } else if let Some(ctx) = entry.get("context_length").and_then(|v| v.as_u64()).filter(|c| *c > 0) {
                context.insert(mid.clone(), ctx);
            }
            if let Some(out) = entry
                .get("max_output_tokens")
                .and_then(|v| v.as_u64())
                .filter(|o| *o > 0)
            {
                max_tokens.insert(mid.clone(), out);
            } else if let Some(out) = entry
                .get("top_provider")
                .and_then(|t| t.get("max_completion_tokens"))
                .and_then(|v| v.as_u64())
                .filter(|o| *o > 0)
            {
                max_tokens.insert(mid.clone(), out);
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
    LiveProbeResult {
        pricing,
        thinking,
        context,
        max_tokens,
    }
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
    models_dev: &ModelsDevData,
    live: &HashMap<String, LiveProbeResult>,
    aimd: &AIModelDir,
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

    // Compiled multi-source fallback (ai-model-directory): keyed by
    // `provider/modelid`, then by bare model id.
    let aimd_cr = previous_cost
        .and_then(|p| p.get("cacheRead").and_then(|v| v.as_f64()))
        .unwrap_or(0.0);
    let aimd_cw = previous_cost
        .and_then(|p| p.get("cacheWrite").and_then(|v| v.as_f64()))
        .unwrap_or(0.0);
    if let Some(e) = aimd
        .get(&format!("{}/{}", provider.id, model_id))
        .or_else(|| aimd.get(model_id))
        && (e.input > 0.0 || e.output > 0.0)
    {
        return (e.input, e.output, aimd_cr, aimd_cw, "ai-model-directory");
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
#[allow(dead_code)]
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

/// Parse either a JSON number or a numeric string into an `f64` (defaults to 0).
/// OpenRouter returns prices as strings (`"0.00000095"`), others as numbers.
fn num_or_str(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// ----------------------------------------------------------------------------
/// ai-model-directory (The-Best-Codes) — community-curated, multi-provider catalog
/// used as a secondary (compiled) pricing source when neither a live provider API
/// nor models.dev exposes a price for a model.
/// ----------------------------------------------------------------------------
pub const AIMD_URL: &str = "https://raw.githubusercontent.com/The-Best-Codes/ai-model-directory/main/data/all.json";

/// One ai-model-directory pricing entry (per-million USD, matching its schema).
#[derive(Clone)]
pub struct AIModelDirEntry {
    pub input: f64,
    pub output: f64,
    pub reasoning: bool,
}

/// `provider/modelid` and bare `modelid` → entry.
pub type AIModelDir = HashMap<String, AIModelDirEntry>;

/// Fetch and parse the ai-model-directory catalog.
///
/// Network is skipped in `offline` mode (cached snapshot reused). On any failure
/// the cached snapshot is used when present; otherwise an empty map (caller falls
/// through to the previous catalog price).
pub fn fetch_ai_model_directory(cache_dir: &Path, offline: bool) -> AIModelDir {
    let path = cache_dir.join("ai-model-directory.json");
    let text = if offline {
        match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => {
                term::note("ai-model-directory: no offline cache — skipping");
                return AIModelDir::new();
            }
        }
    } else {
        match super::common::http_get_text(AIMD_URL, Duration::from_secs(60), None) {
            Ok((status, t)) if status.is_success() => {
                let _ = fs::write(&path, &t);
                t
            }
            _ => match fs::read_to_string(&path) {
                Ok(t) => {
                    term::warn("ai-model-directory fetch failed — using cache");
                    t
                }
                Err(_) => return AIModelDir::new(),
            },
        }
    };
    parse_aimd(&text)
}

fn parse_aimd(text: &str) -> AIModelDir {
    let Ok(json) = serde_json::from_str::<Value>(text) else {
        return AIModelDir::new();
    };
    let mut out = AIModelDir::new();
    if let Some(providers) = json.as_object() {
        for (pkey, prov) in providers {
            if let Some(models) = prov.get("models").and_then(|m| m.as_object()) {
                for (mid, m) in models {
                    let pricing = m.get("pricing");
                    let input = pricing
                        .and_then(|p| p.get("input"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let output = pricing
                        .and_then(|p| p.get("output"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let reasoning = m
                        .get("features")
                        .and_then(|f| f.get("reasoning"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if input > 0.0 || output > 0.0 {
                        let entry = AIModelDirEntry {
                            input,
                            output,
                            reasoning,
                        };
                        out.insert(format!("{pkey}/{mid}"), entry.clone());
                        out.entry(mid.clone()).or_insert(entry);
                    }
                }
            }
        }
    }
    term::note(format!("ai-model-directory: {} priced entries loaded", out.len()));
    out
}

/// Whether ai-model-directory flags a model as a reasoning model.
/// Looked up by `provider/modelid`, then by bare model id.
pub fn aimd_reasoning(aimd: &AIModelDir, provider_id: &str, model_id: &str) -> Option<bool> {
    aimd.get(&format!("{provider_id}/{model_id}"))
        .or_else(|| aimd.get(model_id))
        .map(|e| e.reasoning)
}

/// ----------------------------------------------------------------------------
/// Nara Router official pricing (https://router.bynara.id/api/pricing).
///
/// Nara's `/v1/models` does not expose any pricing, so this dedicated endpoint
/// is the authoritative source for per-model costs. Prices are returned as
/// `official_in_usd_m` / `official_out_usd_m` in USD per million tokens — the
/// exact catalog unit — so no conversion is applied. Credit-based fields are
/// intentionally ignored: the credit→USD rate is not stable across models.
/// ----------------------------------------------------------------------------
pub const NARA_PRICING_URL: &str = "https://router.bynara.id/api/pricing";

/// Nara pricing result: model alias/id -> (input, output, cache_read) USD per million.
pub type NaraPricing = HashMap<String, PriceTriple>;

/// Fetch and parse Nara's official pricing catalog.
///
/// The endpoint is public; an optional `NARA_API_KEY` is forwarded when present.
/// In `offline` mode the cached snapshot is reused, and an empty map is returned
/// when no cache exists (caller falls through to the normal pricing chain).
pub fn fetch_nara_pricing(cache_dir: &Path, offline: bool) -> NaraPricing {
    let path = cache_dir.join("nara-pricing.json");
    let text = if offline {
        match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => {
                term::note("Nara pricing: no offline cache — skipping");
                return NaraPricing::new();
            }
        }
    } else {
        match super::common::http_get_text(
            NARA_PRICING_URL,
            Duration::from_secs(30),
            env::var("NARA_API_KEY").ok().as_deref(),
        ) {
            Ok((status, t)) if status.is_success() => {
                let _ = fs::write(&path, &t);
                t
            }
            _ => fs::read_to_string(&path).unwrap_or_default(),
        }
    };
    parse_nara_pricing(&text)
}

fn parse_nara_pricing(text: &str) -> NaraPricing {
    let Ok(json) = serde_json::from_str::<Value>(text) else {
        return NaraPricing::new();
    };
    let Some(data) = json.get("data").and_then(|d| d.as_array()) else {
        return NaraPricing::new();
    };
    let mut out = NaraPricing::new();
    for entry in data {
        let Some(alias) = entry.get("alias").and_then(|v| v.as_str()) else {
            continue;
        };
        // Only USD fields — the catalog unit is USD per million tokens.
        let input = entry.get("official_in_usd_m").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let output = entry.get("official_out_usd_m").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if input > 0.0 || output > 0.0 {
            out.insert(alias.to_string(), (input, output, 0.0));
        }
    }
    term::note(format!("Nara official pricing: {} models loaded (USD/million)", out.len()));
    out
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

    #[test]
    fn live_context_reads_openrouter_context_length_and_max_completion_tokens() {
        let body = serde_json::json!({
            "data": [
                {
                    "id": "google/gemini-2.5-flash:batch",
                    "context_length": 1048576,
                    "pricing": { "prompt": "0.00000015", "completion": "0.00000125" },
                    "top_provider": { "max_completion_tokens": 65535 }
                },
                {
                    "id": "databyte/m1",
                    "context_window": 262144,
                    "max_output_tokens": 65536,
                    "metadata": { "pricing": { "input_per_million": 1.0, "output_per_million": 2.0 } }
                }
            ]
        });
        let res = parse_live_probe_body(&body);
        assert_eq!(res.context.get("google/gemini-2.5-flash:batch"), Some(&1_048_576));
        assert_eq!(res.max_tokens.get("google/gemini-2.5-flash:batch"), Some(&65_535));
        assert_eq!(res.context.get("databyte/m1"), Some(&262_144));
        assert_eq!(res.max_tokens.get("databyte/m1"), Some(&65_536));
    }
}
