//! Optional pricing enrichment from models.dev and live provider APIs.
//!
//! After generating/merging model catalogs, this module can optionally update
//! zero-priced models with actual pricing data from:
//! - [`models.dev/api.json`](https://models.dev/api.json) — curated pricing DB
//! - Live provider `/v1/models` endpoints (OpenAI-compatible with metadata)

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

// ---------------------------------------------------------------------------
// provider-specific API pricing fetchers
// ---------------------------------------------------------------------------

/// Try to fetch pricing from a provider's OpenAI-compatible `/v1/models` endpoint.
///
/// Returns a map of model_id → (input, output, cache_read).
fn fetch_live_provider_pricing(_provider_id: &str, base_url: &str) -> HashMap<String, (f64, f64, f64)> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let Ok(resp) = reqwest::blocking::get(&url) else {
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
            // Try extended format: metadata.pricing.{input,output}_per_million
            if let Some(pricing) = entry.get("metadata").and_then(|m| m.get("pricing")) {
                let inp = pricing.get("input_per_million").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let outp = pricing
                    .get("output_per_million")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let cached = pricing
                    .get("cached_input_per_million")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if inp > 0.0 || outp > 0.0 {
                    out.insert(mid, (inp, outp, cached));
                }
            }
        }
    }
    out
}

/// Providers whose `/v1/models` endpoint returns `metadata.pricing`.
const LIVE_PRICING_PROVIDERS: &[(&str, &str)] = &[("neuralwatt", "https://api.neuralwatt.com/v1")];

// ---------------------------------------------------------------------------
// enrichment logic
// ---------------------------------------------------------------------------

/// Check if a model cost object is all zeros.
fn is_zero_priced(cost: &Value) -> bool {
    let input = cost.get("input").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let output = cost.get("output").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let cache_read = cost.get("cacheRead").and_then(|v| v.as_f64()).unwrap_or(0.0);
    input == 0.0 && output == 0.0 && cache_read == 0.0
}

/// Write a model JSON file with updated pricing for one model.
fn update_model_cost(
    path: &Path,
    model_id: &str,
    input_price: f64,
    output_price: f64,
    cache_read: f64,
    source: &str,
) -> Result<bool> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut json: Value = serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;

    let Some(model_obj) = json.get_mut(model_id) else {
        return Ok(false);
    };
    let Some(cost) = model_obj.get_mut("cost") else {
        return Ok(false);
    };

    if !is_zero_priced(cost) {
        return Ok(false);
    }

    cost["input"] = json_num(input_price);
    cost["output"] = json_num(output_price);
    cost["cacheRead"] = json_num(cache_read);

    let pretty = serde_json::to_string_pretty(&json).with_context(|| format!("serialize {}", path.display()))?;
    fs::write(path, format!("{pretty}\n")).with_context(|| format!("write {}", path.display()))?;

    println!("  ✓ {model_id}: ${input_price}/in ${output_price}/out ${cache_read}/cache ({source})");
    Ok(true)
}

fn json_num(v: f64) -> Value {
    serde_json::Number::from_f64(v)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// Look up model pricing in the models.dev dataset.
fn lookup_models_dev(
    model_id: &str,
    provider_id: &str,
    models_dev: &HashMap<String, Value>,
) -> Option<(f64, f64, f64)> {
    let candidates = provider_candidates(provider_id);

    for key in &candidates {
        if let Some(prov) = models_dev.get(*key)
            && let Some(models) = prov.get("models").and_then(|m| m.as_object())
            && let Some(model) = models.get(model_id)
            && let Some(cost) = model.get("cost")
        {
            let inp = cost.get("input").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let outp = cost.get("output").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let cached = cost
                .get("cache_read")
                .or_else(|| cost.get("cacheRead"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if inp > 0.0 || outp > 0.0 {
                return Some((inp, outp, cached));
            }
        }
    }
    None
}

/// Map our provider IDs to models.dev provider keys.
fn provider_candidates(ours: &str) -> Vec<&'static str> {
    match ours {
        "deepseek" => vec!["deepseek"],
        "fireworks" => vec!["fireworks-ai", "fireworks"],
        "google" => vec!["google"],
        "groq" => vec!["groq"],
        "mistral" => vec!["mistral"],
        "moonshotai" | "moonshotai-cn" => vec!["moonshotai", "moonshot"],
        "openai" | "openai-codex" => vec!["openai"],
        "together" => vec!["together"],
        "nvidia" => vec!["nvidia"],
        "xai" => vec!["x-ai", "xai"],
        "xiaomi" | "xiaomi-token-plan-ams" | "xiaomi-token-plan-cn" | "xiaomi-token-plan-sgp" => {
            vec!["xiaomi"]
        }
        "zai" | "zai-coding-cn" => vec!["zhipuai", "zai"],
        "neuralwatt" => vec![],
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// main entry point
// ---------------------------------------------------------------------------

/// Enrich all model JSON files in `models_dir` with real pricing.
pub fn run_enrich(models_dir: &Path) -> Result<()> {
    println!("\n=== Enriching model pricing ===");

    // 1. Fetch models.dev pricing database
    let models_dev = fetch_models_dev()?;

    // 2. Fetch live provider pricing
    let mut live_pricing: HashMap<String, HashMap<String, (f64, f64, f64)>> = HashMap::new();
    for (provider_id, base_url) in LIVE_PRICING_PROVIDERS {
        let prices = fetch_live_provider_pricing(provider_id, base_url);
        if !prices.is_empty() {
            println!("  Fetched {} prices from {provider_id} live API", prices.len());
            live_pricing.insert(provider_id.to_string(), prices);
        }
    }

    // 3. Enrich each file
    let mut total_updated = 0usize;
    let mut total_files = 0usize;

    for entry in fs::read_dir(models_dir).context("read models directory")? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".json") || name == "index.json" {
            continue;
        }

        let rust_mod = name.strip_suffix(".json").unwrap_or(name);
        let provider_id = rust_mod.replace('_', "-");

        let n = enrich_file(&path, &models_dev, &live_pricing, &provider_id)?;
        if n > 0 {
            println!("  {}: enriched {n} models", provider_id);
            total_updated += n;
            total_files += 1;
        }
    }

    if total_updated > 0 {
        println!("\n✅ Enriched {total_updated} models across {total_files} provider files");
    } else {
        println!("\n📋 No zero-priced models found to enrich");
    }

    Ok(())
}

/// Enrich zero-priced models in a single provider catalog file.
fn enrich_file(
    path: &Path,
    models_dev: &HashMap<String, Value>,
    live_pricing: &HashMap<String, HashMap<String, (f64, f64, f64)>>,
    provider_id: &str,
) -> Result<usize> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let json: Value = serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;

    let Some(models) = json.as_object() else {
        return Ok(0);
    };

    let mut updated = 0usize;
    for model_id in models.keys() {
        let cost = &models[model_id]["cost"];
        if !is_zero_priced(cost) {
            continue;
        }

        // 1. Try models.dev pricing
        if let Some((inp, outp, cached)) = lookup_models_dev(model_id, provider_id, models_dev)
            && update_model_cost(path, model_id, inp, outp, cached, "models.dev")?
        {
            updated += 1;
            continue;
        }

        // 2. Try live provider pricing
        if let Some(prices) = live_pricing.get(provider_id)
            && let Some(&(inp, outp, cached)) = prices.get(model_id)
            && update_model_cost(path, model_id, inp, outp, cached, "live API")?
        {
            updated += 1;
        }
    }

    Ok(updated)
}

fn fetch_models_dev() -> Result<HashMap<String, Value>> {
    let url = "https://models.dev/api.json";
    println!("  Fetching {url}...");

    let resp = reqwest::blocking::get(url).context("fetch models.dev/api.json")?;
    if !resp.status().is_success() {
        anyhow::bail!("{url} returned {}", resp.status());
    }

    let text = resp.text().context("read models.dev/api.json body")?;
    let root: HashMap<String, Value> = serde_json::from_str(&text).context("parse models.dev/api.json")?;

    println!("  Got {} providers from models.dev", root.len());
    Ok(root)
}
