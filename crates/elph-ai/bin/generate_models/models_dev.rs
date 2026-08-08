//! Fetch and index [models.dev](https://models.dev) catalog data.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::term;

const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";

/// How long a cached models.dev snapshot is considered fresh (24 hours).
pub const MODELS_DEV_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Cached models.dev root: provider_key → provider object (with `models` map).
///
/// Ordered so cross-provider fuzzy lookups (`find_model_fuzzy`) always pick the
/// same donor entry, keeping generated catalogs byte-stable across runs.
pub type ModelsDevRoot = BTreeMap<String, Value>;

/// Fetch models.dev api.json (or load offline cache).
///
/// - `offline`: use the cached snapshot only (error if missing).
/// - `force`: bypass the freshness check and always re-fetch.
/// - Otherwise: use the cache when it is younger than [`MODELS_DEV_CACHE_TTL`];
///   on a fetch failure, fall back to the cached snapshot when one exists.
pub fn load_models_dev(cache_dir: &Path, offline: bool, force: bool) -> Result<ModelsDevRoot> {
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

    // Freshness check: reuse the cache when it is young enough and not forced.
    if !force && cache_is_fresh(&cache_path) {
        let text = fs::read_to_string(&cache_path).with_context(|| format!("read {}", cache_path.display()))?;
        let root: ModelsDevRoot = serde_json::from_str(&text).context("parse cached models.dev api.json")?;
        term::info(format!(
            "Loaded {} providers from fresh cache {} (age < 24h)",
            root.len(),
            cache_path.display()
        ));
        return Ok(root);
    }

    term::fetch(format!("Fetching {MODELS_DEV_API_URL}…"));
    let fetch = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("build HTTP client")?
        .get(MODELS_DEV_API_URL)
        .send();

    match fetch {
        Ok(resp) if resp.status().is_success() => {
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
        Ok(resp) => {
            // Fetch failed (non-2xx): fall back to cache when available.
            if cache_path.is_file() {
                let text = fs::read_to_string(&cache_path).with_context(|| format!("read {}", cache_path.display()))?;
                let root: ModelsDevRoot = serde_json::from_str(&text).context("parse cached models.dev api.json")?;
                term::warn(format!(
                    "{MODELS_DEV_API_URL} returned {} — using cached snapshot ({} providers)",
                    resp.status(),
                    root.len()
                ));
                Ok(root)
            } else {
                bail!("{MODELS_DEV_API_URL} returned {}", resp.status());
            }
        }
        Err(e) => {
            // Network failure: fall back to cache when available.
            if cache_path.is_file() {
                let text = fs::read_to_string(&cache_path).with_context(|| format!("read {}", cache_path.display()))?;
                let root: ModelsDevRoot = serde_json::from_str(&text).context("parse cached models.dev api.json")?;
                term::warn(format!(
                    "fetch {MODELS_DEV_API_URL} failed ({e}) — using cached snapshot ({} providers)",
                    root.len()
                ));
                Ok(root)
            } else {
                Err(e).context("fetch models.dev/api.json")
            }
        }
    }
}

/// True when the cache file exists and is younger than [`MODELS_DEV_CACHE_TTL`].
fn cache_is_fresh(cache_path: &Path) -> bool {
    let Ok(meta) = fs::metadata(cache_path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    modified.elapsed().is_ok_and(|age| age < MODELS_DEV_CACHE_TTL)
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
    // Providers disagree on model id casing (`Kimi-K3` vs `kimi-k3`); fall back
    // to a case-insensitive sweep so gateways still inherit limits/modalities.
    for (pkey, prov) in root {
        if let Some(models) = prov.get("models").and_then(|m| m.as_object()) {
            for (mid, m) in models {
                if mid.eq_ignore_ascii_case(model_id) {
                    return Some((pkey.clone(), m.clone()));
                }
            }
        }
    }
    None
}
