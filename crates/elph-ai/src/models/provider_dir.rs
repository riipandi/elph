//! Registration of the on-disk providers directory (`CONFIG_DIR/providers`).
//!
//! Scanning only lists files — catalogs are parsed lazily by [`super::catalog`] on first use.
//! The scan exists to discover **disk-only** providers (files without an embedded seed), which
//! the runtime collection has to register as extra providers.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::sync::{PoisonError, RwLock};

use crate::types::Model;

use super::catalog::{builtin_catalog, builtin_provider_ids, set_provider_catalog_dir};

static CUSTOM_PROVIDER_IDS: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Register `dir` as the catalog source and re-scan it for disk-only providers.
///
/// Safe to call repeatedly (bootstrap, `/reload`, session resolve): cached catalogs are dropped
/// so edited files take effect. A missing directory clears the registration. Returns the number
/// of provider catalog files found.
pub fn install_provider_catalog_dir(dir: &Path) -> Result<usize, String> {
    if !dir.is_dir() {
        set_provider_catalog_dir(None);
        set_custom_provider_ids(Vec::new());
        return Ok(0);
    }

    let mut found = 0usize;
    let mut custom = Vec::new();
    let builtin: BTreeSet<&str> = builtin_provider_ids().iter().copied().collect();
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem.eq_ignore_ascii_case("index") {
            continue;
        }
        found += 1;
        if !builtin.contains(stem) {
            custom.push(stem.to_string());
        }
    }
    custom.sort();

    set_provider_catalog_dir(Some(dir.to_path_buf()));
    set_custom_provider_ids(custom);
    Ok(found)
}

/// Provider ids that exist only as files on disk (no embedded seed), sorted.
pub fn custom_provider_ids() -> Vec<String> {
    CUSTOM_PROVIDER_IDS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Catalogs of disk-only providers, parsed on demand. Unparsable files are skipped.
pub fn custom_provider_catalogs() -> HashMap<String, Vec<Model>> {
    custom_provider_ids()
        .into_iter()
        .filter_map(|id| {
            let models = builtin_catalog(&id);
            if models.is_empty() {
                None
            } else {
                Some((id, models.as_ref().clone()))
            }
        })
        .collect()
}

/// Builtin ∪ disk-only provider ids, sorted.
pub fn all_provider_ids() -> Vec<String> {
    let mut ids: BTreeSet<String> = builtin_provider_ids().iter().map(|id| (*id).to_string()).collect();
    ids.extend(custom_provider_ids());
    ids.into_iter().collect()
}

fn set_custom_provider_ids(ids: Vec<String>) {
    *CUSTOM_PROVIDER_IDS.write().unwrap_or_else(PoisonError::into_inner) = ids;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_catalog(dir: &Path, provider: &str) {
        let json = format!(
            r#"{{
              "m1": {{
                "id": "m1",
                "name": "M1",
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
    fn scan_separates_custom_providers_from_builtins() {
        let _guard = super::super::catalog::TEST_DIR_GUARD
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let tmp = tempfile::tempdir().expect("tempdir");
        write_catalog(tmp.path(), "anthropic");
        write_catalog(tmp.path(), "my-gateway");
        fs::write(tmp.path().join("index.json"), "[]").expect("write index");

        let found = install_provider_catalog_dir(tmp.path()).expect("install");
        assert_eq!(found, 2, "index.json is not a provider catalog");
        assert_eq!(custom_provider_ids(), vec!["my-gateway".to_string()]);

        let catalogs = custom_provider_catalogs();
        assert_eq!(catalogs.len(), 1);
        assert_eq!(catalogs["my-gateway"].len(), 1);
        assert!(all_provider_ids().contains(&"my-gateway".to_string()));
        assert!(all_provider_ids().contains(&"anthropic".to_string()));

        install_provider_catalog_dir(Path::new("/nonexistent-provider-dir")).expect("clear");
        assert!(custom_provider_ids().is_empty());
    }

    #[test]
    fn missing_dir_clears_registration() {
        let _guard = super::super::catalog::TEST_DIR_GUARD
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let found = install_provider_catalog_dir(Path::new("/nonexistent-provider-dir")).expect("install");
        assert_eq!(found, 0);
        assert!(custom_provider_ids().is_empty());
        assert!(super::super::catalog::provider_catalog_dir().is_none());
    }
}
