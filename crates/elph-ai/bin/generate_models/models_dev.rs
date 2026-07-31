//! Fetch and index [models.dev](https://models.dev) catalog data.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::term;

const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";

/// Cached models.dev root: provider_key → provider object (with `models` map).
pub type ModelsDevRoot = HashMap<String, Value>;

/// Fetch models.dev api.json (or load offline cache).
pub fn load_models_dev(cache_dir: &Path, offline: bool) -> Result<ModelsDevRoot> {
    fs::create_dir_all(cache_dir).with_context(|| format!("create cache dir {}", cache_dir.display()))?;
    let cache_path = cache_dir.join("api.json");

    if offline {
        if !cache_path.is_file() {
            bail!(
                "offline mode requires cached models.dev at {}\n  run once online without --offline",
                cache_path.display()
            );
        }
        let text = fs::read_to_string(&cache_path).with_context(|| format!("read {}", cache_path.display()))?;
        let root: ModelsDevRoot = serde_json::from_str(&text).context("parse cached models.dev api.json")?;
        term::info(format!("Loaded {} providers from cache {}", root.len(), cache_path.display()));
        return Ok(root);
    }

    term::fetch(format!("Fetching {MODELS_DEV_API_URL}…"));
    let resp = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("build HTTP client")?
        .get(MODELS_DEV_API_URL)
        .send()
        .context("fetch models.dev/api.json")?;
    if !resp.status().is_success() {
        bail!("{MODELS_DEV_API_URL} returned {}", resp.status());
    }
    let text = resp.text().context("read models.dev body")?;
    let root: ModelsDevRoot = serde_json::from_str(&text).context("parse models.dev api.json")?;
    fs::write(&cache_path, &text).with_context(|| format!("write {}", cache_path.display()))?;
    term::fetch(format!(
        "Got {} providers from models.dev (cached → {})",
        root.len(),
        cache_path.display()
    ));
    Ok(root)
}

/// Default cache directory under the models output dir.
pub fn default_cache_dir(models_dir: &Path) -> PathBuf {
    models_dir.join(".cache").join("models.dev")
}

/// Look up a model object in models.dev under any of the candidate provider keys.
pub fn find_model<'a>(root: &'a ModelsDevRoot, provider_keys: &[&str], model_id: &str) -> Option<&'a Value> {
    for key in provider_keys {
        if let Some(prov) = root.get(*key)
            && let Some(models) = prov.get("models").and_then(|m| m.as_object())
        {
            if let Some(m) = models.get(model_id) {
                return Some(m);
            }
            // Case-insensitive id match
            for (mid, m) in models {
                if mid.eq_ignore_ascii_case(model_id) {
                    return Some(m);
                }
            }
        }
    }
    None
}

/// All models for the first matching models.dev provider key.
pub fn models_for_provider_keys<'a>(
    root: &'a ModelsDevRoot,
    provider_keys: &[&'a str],
) -> Option<(&'a str, &'a serde_json::Map<String, Value>)> {
    for key in provider_keys {
        if let Some(prov) = root.get(*key)
            && let Some(models) = prov.get("models").and_then(|m| m.as_object())
            && !models.is_empty()
        {
            return Some((*key, models));
        }
    }
    None
}

/// Try match by stripping vendor prefix (`anthropic/claude-…` → `claude-…`) across all providers.
pub fn find_model_fuzzy(root: &ModelsDevRoot, model_id: &str) -> Option<(String, Value)> {
    if let Some(m) = find_model_any(root, model_id) {
        return Some(m);
    }
    if let Some((_, rest)) = model_id.split_once('/') {
        return find_model_any(root, rest);
    }
    None
}

fn find_model_any(root: &ModelsDevRoot, model_id: &str) -> Option<(String, Value)> {
    for (pkey, prov) in root {
        if let Some(models) = prov.get("models").and_then(|m| m.as_object())
            && let Some(m) = models.get(model_id)
        {
            return Some((pkey.clone(), m.clone()));
        }
    }
    None
}
