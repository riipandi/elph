//! Fetch and index [models.dev](https://models.dev) catalog data.
//!
//! Three endpoints are fetched and merged into one index:
//! - `api.json`      — nested `provider → {models}`; authoritative for `cost`,
//!   `reasoning_options`, `modalities`, `limit`.
//! - `models.json`   — flat `provider/modelid → model`; authoritative for `description`,
//!   `knowledge` (cutoff), `benchmarks`, `release_date`, `weights`.
//! - `catalog.json`  — same as `models.json` wrapped in `{ "models", "providers" }`;
//!   used to back-fill the rich index (wins on conflict).
//!
//! The nested `api` tree stays the source of truth for cost/limits; the `rich`
//! tree adds the human-readable metadata that `api.json` omits.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::term;

const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";
const MODELS_DEV_MODELS_URL: &str = "https://models.dev/models.json";
const MODELS_DEV_CATALOG_URL: &str = "https://models.dev/catalog.json";

/// How long a cached models.dev snapshot is considered fresh (24 hours).
pub const MODELS_DEV_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Cached models.dev api.json root: provider_key → provider object (with `models` map).
///
/// Ordered so cross-provider fuzzy lookups (`find_model_fuzzy`) always pick the
/// same donor entry, keeping generated catalogs byte-stable across runs.
pub type ModelsDevRoot = BTreeMap<String, Value>;

/// Merged models.dev index.
pub struct ModelsDevData {
    /// Nested provider → { models:{...} } (from `api.json`). Holds cost,
    /// reasoning_options, modalities, and limit.
    pub api: ModelsDevRoot,
    /// Flat `provider/modelid` → model (from `models.json` / `catalog.json`).
    /// Holds description, knowledge, benchmarks, release_date, weights. No pricing.
    pub rich: BTreeMap<String, Value>,
}

impl ModelsDevData {
    /// Look up a rich (metadata-complete) model entry.
    ///
    /// Tries the exact `provider/modelid` key, then falls back to scanning for a
    /// model whose `id` field equals `model_id` (handles gateway ids such as
    /// `anthropic/claude-3.5-sonnet` stored under `openrouter/anthropic/...`).
    pub fn rich_model(&self, provider: &str, model_id: &str) -> Option<&Value> {
        let key = format!("{provider}/{model_id}");
        if let Some(v) = self.rich.get(&key) {
            return Some(v);
        }
        self.rich
            .values()
            .find(|v| v.get("id").and_then(|i| i.as_str()) == Some(model_id))
    }

    /// Find a models.dev entry by searching all providers' models for a keyword
    /// match against the model id (case-insensitive). Used as a fallback when
    /// a gateway-preserved ID like `tencent-hy3-free` has no direct match but
    /// the underlying family model (e.g. `tencent/hy3`) exists on models.dev.
    pub fn find_model_by_keyword(&self, keyword: &str) -> Option<Value> {
        let kw = keyword.to_ascii_lowercase();
        for prov in self.api.values() {
            if let Some(models) = prov.get("models").and_then(|m| m.as_object()) {
                for (_mid, m) in models {
                    let mid_lower = _mid.to_ascii_lowercase();
                    if mid_lower.contains(&kw) {
                        return Some(m.clone());
                    }
                }
            }
        }
        None
    }
}

/// Fetch all three models.dev endpoints (or load offline cache).
///
/// - `offline`: use the cached snapshots only (error if any is missing).
/// - `force`: bypass the freshness check and always re-fetch.
/// - Otherwise: reuse each cache file when younger than [`MODELS_DEV_CACHE_TTL`];
///   on a fetch failure for a file, fall back to its cached snapshot.
pub fn load_models_dev(cache_dir: &Path, offline: bool, force: bool) -> Result<ModelsDevData> {
    fs::create_dir_all(cache_dir).with_context(|| format!("create cache dir {}", cache_dir.display()))?;

    let api = fetch_json(MODELS_DEV_API_URL, &cache_dir.join("api.json"), offline, force, "api.json")?;
    let models = fetch_json(
        MODELS_DEV_MODELS_URL,
        &cache_dir.join("models.json"),
        offline,
        force,
        "models.json",
    )?;
    let catalog = fetch_json(
        MODELS_DEV_CATALOG_URL,
        &cache_dir.join("catalog.json"),
        offline,
        force,
        "catalog.json",
    )?;

    let api: ModelsDevRoot = serde_json::from_value(api).context("parse models.dev api.json")?;
    let rich = build_rich(&models, &catalog);

    term::info(format!(
        "Loaded {} providers (api.json) + {} rich model entries (models.json/catalog.json)",
        api.len(),
        rich.len()
    ));

    Ok(ModelsDevData { api, rich })
}

/// Fetch one models.dev endpoint with caching, TTL, and fallback.
fn fetch_json(url: &str, cache_path: &Path, offline: bool, force: bool, label: &str) -> Result<Value> {
    if offline {
        if !cache_path.is_file() {
            bail!(
                "offline mode requires cached {label} at {}\n  run once online without --offline",
                cache_path.display()
            );
        }
        let text = fs::read_to_string(cache_path).with_context(|| format!("read {}", cache_path.display()))?;
        return serde_json::from_str(&text).with_context(|| format!("parse cached {label}"));
    }

    if !force && cache_is_fresh(cache_path) {
        let text = fs::read_to_string(cache_path).with_context(|| format!("read {}", cache_path.display()))?;
        term::fetch(format!("Loaded {label} from fresh cache {} (age < 24h)", cache_path.display()));
        return serde_json::from_str(&text).with_context(|| format!("parse cached {label}"));
    }

    term::fetch(format!("Fetching {url}…"));
    let fetch = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("build HTTP client")?
        .get(url)
        .send();

    match fetch {
        Ok(resp) if resp.status().is_success() => {
            let text = resp.text().context("read models.dev body")?;
            fs::write(cache_path, &text).with_context(|| format!("write {}", cache_path.display()))?;
            term::fetch(format!("Got {label} (cached → {})", cache_path.display()));
            serde_json::from_str(&text).with_context(|| format!("parse {label}"))
        }
        Ok(resp) => {
            if cache_path.is_file() {
                term::warn(format!("{url} returned {} — using cached {label}", resp.status()));
                let text = fs::read_to_string(cache_path).with_context(|| format!("read {}", cache_path.display()))?;
                serde_json::from_str(&text).with_context(|| format!("parse cached {label}"))
            } else {
                bail!("{url} returned {}", resp.status());
            }
        }
        Err(e) => {
            if cache_path.is_file() {
                term::warn(format!("fetch {url} failed ({e}) — using cached {label}"));
                let text = fs::read_to_string(cache_path).with_context(|| format!("read {}", cache_path.display()))?;
                serde_json::from_str(&text).with_context(|| format!("parse cached {label}"))
            } else {
                Err(e).context(format!("fetch {url}"))
            }
        }
    }
}

/// Merge `models.json` (flat) and `catalog.json` (`{"models": flat}`) into the rich index.
/// `catalog.json` wins on key conflicts (identical data, catalog is the canonical export).
fn build_rich(models_json: &Value, catalog_json: &Value) -> BTreeMap<String, Value> {
    let mut rich = BTreeMap::new();
    if let Some(obj) = models_json.as_object() {
        for (k, v) in obj {
            rich.insert(k.clone(), v.clone());
        }
    }
    if let Some(models) = catalog_json.get("models").and_then(|m| m.as_object()) {
        for (k, v) in models {
            rich.insert(k.clone(), v.clone());
        }
    }
    rich
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
pub fn find_model<'a>(root: &'a ModelsDevData, provider_keys: &[&str], model_id: &str) -> Option<&'a Value> {
    for key in provider_keys {
        if let Some(prov) = root.api.get(*key)
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
    root: &'a ModelsDevData,
    provider_keys: &[&'a str],
) -> Option<(&'a str, &'a serde_json::Map<String, Value>)> {
    for key in provider_keys {
        if let Some(prov) = root.api.get(*key)
            && let Some(models) = prov.get("models").and_then(|m| m.as_object())
            && !models.is_empty()
        {
            return Some((*key, models));
        }
    }
    None
}

/// Try match by stripping vendor prefix (`anthropic/claude-…` → `claude-…`) across all providers.
pub fn find_model_fuzzy(root: &ModelsDevData, model_id: &str) -> Option<(String, Value)> {
    if let Some(m) = find_model_any(root, model_id) {
        return Some(m);
    }
    if let Some((_, rest)) = model_id.split_once('/') {
        return find_model_any(root, rest);
    }
    None
}

fn find_model_any(root: &ModelsDevData, model_id: &str) -> Option<(String, Value)> {
    for (pkey, prov) in &root.api {
        if let Some(models) = prov.get("models").and_then(|m| m.as_object())
            && let Some(m) = models.get(model_id)
        {
            return Some((pkey.clone(), m.clone()));
        }
    }
    // Providers disagree on model id casing (`Kimi-K3` vs `kimi-k3`); fall back
    // to a case-insensitive sweep so gateways still inherit limits/modalities.
    for (pkey, prov) in &root.api {
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

/// Distinct provider keys present in the rich index (used for diagnostics/tests).
#[allow(dead_code)]
pub fn rich_provider_keys(root: &ModelsDevData) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for k in root.rich.keys() {
        if let Some((p, _)) = k.split_once('/') {
            set.insert(p.to_string());
        }
    }
    set
}
