//! Unpack embedded provider catalogs into `CONFIG_DIR/providers/PROVIDER_ID.json`.

use crate::utils::path::AppPaths;
use anyhow::{Context, Result};
use elph_ai::embedded_provider_json;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Ensure every built-in provider catalog exists under `providers/` (kebab-case ids).
///
/// Existing files are never overwritten so user overrides / custom providers win.
pub struct ProvidersUnpack;

impl ProvidersUnpack {
    pub fn ensure<P: AppPaths>(paths: &P) -> Result<ProvidersUnpackReport> {
        let dir = paths.providers_dir();
        fs::create_dir_all(&dir).with_context(|| format!("create providers dir {}", dir.display()))?;

        let mut written = 0usize;
        let mut skipped = 0usize;
        for (provider_id, json) in embedded_provider_json() {
            let path = dir.join(format!("{provider_id}.json"));
            if path.exists() {
                skipped += 1;
                continue;
            }
            write_pretty_json(&path, json).with_context(|| format!("write provider catalog {}", path.display()))?;
            written += 1;
        }

        Ok(ProvidersUnpackReport { written, skipped })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProvidersUnpackReport {
    pub written: usize,
    pub skipped: usize,
}

fn write_pretty_json(path: &Path, raw: &str) -> Result<()> {
    // Re-serialize for stable pretty output; fall back to raw bytes if parse fails.
    let body = match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value) => {
            let mut s = serde_json::to_string_pretty(&value)?;
            s.push('\n');
            s
        }
        Err(_) => {
            let mut s = raw.to_string();
            if !s.ends_with('\n') {
                s.push('\n');
            }
            s
        }
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    file.write_all(body.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Paths;
    use crate::utils::path::AppPaths;

    #[test]
    fn unpack_writes_missing_only() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = Paths::from_dirs(tmp.path().join("config"), tmp.path().join("data"), tmp.path().join("project"));
        fs::create_dir_all(paths.providers_dir()).unwrap();

        let first = ProvidersUnpack::ensure(&paths).expect("first unpack");
        assert!(first.written > 0);
        assert_eq!(first.skipped, 0);

        let anthropic = paths.providers_dir().join("anthropic.json");
        assert!(anthropic.is_file());
        let original = fs::read_to_string(&anthropic).unwrap();
        // User override marker
        fs::write(&anthropic, "{\n  \"user\": true\n}\n").unwrap();

        let second = ProvidersUnpack::ensure(&paths).expect("second unpack");
        assert_eq!(second.written, 0);
        assert!(second.skipped > 0);
        assert_eq!(fs::read_to_string(&anthropic).unwrap(), "{\n  \"user\": true\n}\n");
        assert_ne!(original, "{\n  \"user\": true\n}\n");
    }

    #[test]
    fn provider_ids_are_kebab_case() {
        for (id, _) in embedded_provider_json() {
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "provider id must be kebab-case: {id}"
            );
            assert!(!id.contains('_'), "provider id must not use underscores: {id}");
        }
    }

    #[test]
    fn disk_override_changes_get_builtin_model() {
        use elph_ai::{get_builtin_model, set_disk_catalog_overrides};
        use std::collections::HashMap;

        // Start clean so other tests don't pollute.
        set_disk_catalog_overrides(HashMap::new());
        let baseline = get_builtin_model("anthropic", "claude-haiku-4-5").expect("embedded haiku");

        let mut models = elph_ai::get_builtin_models("anthropic");
        if let Some(m) = models.iter_mut().find(|m| m.id == "claude-haiku-4-5") {
            m.name = "Haiku Override".into();
        }
        let mut overlay = HashMap::new();
        overlay.insert("anthropic".into(), models);
        set_disk_catalog_overrides(overlay);

        let overridden = get_builtin_model("anthropic", "claude-haiku-4-5").expect("overridden");
        assert_eq!(overridden.name, "Haiku Override");
        assert_ne!(baseline.name, "Haiku Override");

        set_disk_catalog_overrides(HashMap::new());
    }
}
