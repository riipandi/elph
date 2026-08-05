//! Lazy builtin model catalogs.
//!
//! A catalog is resolved the first time a provider is touched and cached for the process:
//!
//! 1. embedded seed (compressed, see [`super::embedded`]) — the base list
//! 2. `CONFIG_DIR/providers/<provider>.json` when a directory is registered — overlay by model `id`
//!
//! Registering a directory ([`set_provider_catalog_dir`]) clears the cache, so `/reload`
//! picks up edited files without restarting.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use crate::types::Model;

use super::catalog_json::parse_provider_catalog_json;
use super::embedded::{embedded_provider_ids, embedded_provider_json};

static CATALOG_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);
static CACHE: RwLock<Option<HashMap<String, Arc<Vec<Model>>>>> = RwLock::new(None);

/// Provider ids shipped with the binary (kebab-case, sorted).
pub fn builtin_provider_ids() -> &'static [&'static str] {
    embedded_provider_ids()
}

/// Point catalog loading at a providers directory (`CONFIG_DIR/providers`); `None` disables it.
///
/// Always drops cached catalogs so the next read reflects the new source.
pub fn set_provider_catalog_dir(dir: Option<PathBuf>) {
    *CATALOG_DIR.write().unwrap_or_else(PoisonError::into_inner) = dir;
    invalidate_catalog_cache();
}

/// Currently registered providers directory, if any.
pub fn provider_catalog_dir() -> Option<PathBuf> {
    CATALOG_DIR.read().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Drop every cached catalog (next access re-reads seed + disk).
pub fn invalidate_catalog_cache() {
    *CACHE.write().unwrap_or_else(PoisonError::into_inner) = None;
}

/// Models for a builtin provider, loaded on first use and cached.
///
/// Unknown providers yield an empty list.
pub fn builtin_catalog(provider: &str) -> Arc<Vec<Model>> {
    if let Some(cached) = CACHE
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|cache| cache.get(provider))
    {
        return Arc::clone(cached);
    }

    let models = Arc::new(load_catalog(provider));
    let mut guard = CACHE.write().unwrap_or_else(PoisonError::into_inner);
    let cache = guard.get_or_insert_with(HashMap::new);
    // Another thread may have loaded the same provider meanwhile; keep one shared list.
    Arc::clone(cache.entry(provider.to_string()).or_insert(models))
}

/// Single model from a builtin catalog.
pub fn get_builtin_model(provider: &str, id: &str) -> Option<Model> {
    builtin_catalog(provider).iter().find(|m| m.id == id).cloned()
}

/// Owned copy of a builtin catalog.
pub fn get_builtin_models(provider: &str) -> Vec<Model> {
    builtin_catalog(provider).as_ref().clone()
}

/// Merge `overlay` over `base` by model `id` (overlay wins; extras append). Sorted by id.
pub fn merge_model_lists(base: &[Model], overlay: &[Model]) -> Vec<Model> {
    let mut by_id: HashMap<String, Model> = HashMap::new();
    for m in base {
        by_id.insert(m.id.clone(), m.clone());
    }
    for m in overlay {
        by_id.insert(m.id.clone(), m.clone());
    }
    let mut models: Vec<Model> = by_id.into_values().collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models
}

fn load_catalog(provider: &str) -> Vec<Model> {
    let seed = embedded_provider_json(provider);
    let disk = read_provider_file(provider);

    match (seed, disk) {
        // Untouched unpacked file: identical to the seed, so parse it once.
        (Some(seed_json), Some((path, disk_json))) if seed_json == disk_json => {
            parse_or_warn(&path.to_string_lossy(), &seed_json)
        }
        (Some(seed_json), Some((path, disk_json))) => {
            let base = parse_or_warn(provider, &seed_json);
            let overlay = parse_or_warn(&path.to_string_lossy(), &disk_json);
            if overlay.is_empty() {
                base
            } else {
                merge_model_lists(&base, &overlay)
            }
        }
        (Some(seed_json), None) => parse_or_warn(provider, &seed_json),
        // Custom provider: only a user file exists.
        (None, Some((path, disk_json))) => parse_or_warn(&path.to_string_lossy(), &disk_json),
        (None, None) => Vec::new(),
    }
}

fn read_provider_file(provider: &str) -> Option<(PathBuf, String)> {
    let path = provider_catalog_dir()?.join(format!("{provider}.json"));
    match fs::read_to_string(&path) {
        Ok(body) => Some((path, body)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            log::warn!("read provider catalog {}: {err}", path.display());
            None
        }
    }
}

/// Catalog registration is process-wide: tests that touch it must serialize on this lock.
#[cfg(test)]
pub(crate) static TEST_DIR_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn parse_or_warn(source: &str, json: &str) -> Vec<Model> {
    match parse_provider_catalog_json(json) {
        Ok(models) => models,
        Err(err) => {
            log::warn!("skip provider catalog {source}: {err}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_catalog(dir: &std::path::Path, provider: &str, model_id: &str, name: &str) {
        let json = format!(
            r#"{{
              "{model_id}": {{
                "id": "{model_id}",
                "name": "{name}",
                "api": "openai-completions",
                "provider": "{provider}",
                "baseUrl": "https://example.com",
                "reasoning": false,
                "input": ["text"],
                "cost": {{"input":1,"output":1,"cacheRead":0,"cacheWrite":0}},
                "contextWindow": 8000,
                "maxTokens": 1024
              }}
            }}"#
        );
        fs::write(dir.join(format!("{provider}.json")), json).expect("write catalog");
    }

    #[test]
    fn seed_loads_without_a_catalog_dir() {
        let _guard = TEST_DIR_GUARD.lock().unwrap_or_else(PoisonError::into_inner);
        set_provider_catalog_dir(None);
        let models = builtin_catalog("anthropic");
        assert!(!models.is_empty(), "anthropic seed should load from the embedded frame");
        assert!(models.iter().all(|m| m.provider == "anthropic"));
    }

    #[test]
    fn unknown_provider_is_empty() {
        let _guard = TEST_DIR_GUARD.lock().unwrap_or_else(PoisonError::into_inner);
        set_provider_catalog_dir(None);
        assert!(builtin_catalog("no-such-provider").is_empty());
    }

    #[test]
    fn disk_file_overlays_the_seed() {
        let _guard = TEST_DIR_GUARD.lock().unwrap_or_else(PoisonError::into_inner);
        let tmp = tempfile::tempdir().expect("tempdir");
        set_provider_catalog_dir(None);
        let seed_count = builtin_catalog("anthropic").len();

        write_catalog(tmp.path(), "anthropic", "claude-haiku-4-5", "Haiku Override");
        set_provider_catalog_dir(Some(tmp.path().to_path_buf()));

        let merged = builtin_catalog("anthropic");
        assert_eq!(merged.len(), seed_count, "overlay replaces by id, it does not drop models");
        let haiku = merged.iter().find(|m| m.id == "claude-haiku-4-5").expect("haiku");
        assert_eq!(haiku.name, "Haiku Override");

        set_provider_catalog_dir(None);
        assert_ne!(
            get_builtin_model("anthropic", "claude-haiku-4-5").map(|m| m.name),
            Some("Haiku Override".into())
        );
    }

    #[test]
    fn disk_only_provider_loads_without_a_seed() {
        let _guard = TEST_DIR_GUARD.lock().unwrap_or_else(PoisonError::into_inner);
        let tmp = tempfile::tempdir().expect("tempdir");
        write_catalog(tmp.path(), "my-gateway", "m1", "M1");
        set_provider_catalog_dir(Some(tmp.path().to_path_buf()));

        let models = builtin_catalog("my-gateway");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "m1");

        set_provider_catalog_dir(None);
        assert!(builtin_catalog("my-gateway").is_empty());
    }

    #[test]
    fn invalid_disk_file_falls_back_to_the_seed() {
        let _guard = TEST_DIR_GUARD.lock().unwrap_or_else(PoisonError::into_inner);
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join("anthropic.json"), "{ not json").expect("write");
        set_provider_catalog_dir(Some(tmp.path().to_path_buf()));

        assert!(!builtin_catalog("anthropic").is_empty());
        set_provider_catalog_dir(None);
    }

    #[test]
    fn merge_overlay_replaces_by_id() {
        let base = vec![Model {
            id: "a".into(),
            name: "A".into(),
            api: "openai-completions".into(),
            provider: "x".into(),
            base_url: "https://x".into(),
            reasoning: false,
            thinking_level_map: None,
            input: vec!["text".into()],
            cost: crate::types::ModelCost {
                input: 1.0,
                output: 1.0,
                cache_read: 0.0,
                cache_write: 0.0,
                tiers: None,
            },
            context_window: 1000,
            max_tokens: 100,
            headers: None,
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        }];
        let mut overlay = base.clone();
        overlay[0].name = "A-override".into();
        let merged = merge_model_lists(&base, &overlay);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "A-override");
    }
}
