use crate::utils::path::AppPaths;
use anyhow::{Context, Result};
use elph_agent::fs::write_json_file;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Fallback when `trust.json` has no decision for this folder.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DefaultProjectTrust {
    #[default]
    Ask,
    Always,
    Never,
}

impl DefaultProjectTrust {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

/// Trusted workspace directories (`CONFIG_DIR/trust.json`).
///
/// Paths may use `~`, `$HOME`, or absolute forms. Values are trust flags.
/// `defaultProjectTrust` is global-only (project files do not carry this file).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustStore {
    /// When no directory decision applies: load executable project resources (`always`) or skip (`ask`/`never`).
    /// Interactive TUI startup asks the user when this is `ask`.
    #[serde(default)]
    pub default_project_trust: DefaultProjectTrust,
    #[serde(default)]
    pub directories: BTreeMap<String, bool>,
}

impl TrustStore {
    pub fn ensure<P: AppPaths>(paths: &P) -> Result<()> {
        let path = paths.trust_path();
        if path.exists() {
            return Ok(());
        }

        write_json_file(&path, &Self::default())?;
        Ok(())
    }

    pub fn load<P: AppPaths>(paths: &P) -> Result<Self> {
        let path = paths.trust_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let store: Self = serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
        Ok(store)
    }

    pub fn save<P: AppPaths>(&self, paths: &P) -> Result<()> {
        let path = paths.trust_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
        }
        write_json_file(&path, self).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    /// Canonical storage key for a workspace directory (absolute, expanded).
    pub fn storage_key(cwd: &Path) -> String {
        let expanded = expand_user_path(cwd);
        expanded.canonicalize().unwrap_or(expanded).display().to_string()
    }

    /// Mark `cwd` trusted and persist to `CONFIG_DIR/trust.json`.
    pub fn trust_directory<P: AppPaths>(paths: &P, cwd: &Path) -> Result<String> {
        let mut store = Self::load(paths)?;
        let key = Self::storage_key(cwd);
        store.directories.insert(key.clone(), true);
        store.save(paths)?;
        Ok(key)
    }

    /// Mark `cwd` untrusted and persist to `CONFIG_DIR/trust.json`.
    ///
    /// An explicit `false` decision overrides a trusted ancestor, so a nested
    /// project can opt out without changing the parent's trust decision.
    pub fn untrust_directory<P: AppPaths>(paths: &P, cwd: &Path) -> Result<String> {
        let mut store = Self::load(paths)?;
        let key = Self::storage_key(cwd);
        if !Self::is_trusted_in_store(&store, &key) {
            anyhow::bail!("project is not trusted");
        }
        store.directories.insert(key.clone(), false);
        store.save(paths)?;
        Ok(key)
    }

    /// Whether `cwd` is trusted (exact key or ancestor prefix match).
    pub fn is_trusted<P: AppPaths>(paths: &P, cwd: &Path) -> Result<bool> {
        let store = Self::load(paths)?;
        let key = Self::storage_key(cwd);
        Ok(Self::is_trusted_in_store(&store, &key))
    }

    fn is_trusted_in_store(store: &Self, key: &str) -> bool {
        if let Some(decision) = store.directories.get(key) {
            return *decision;
        }
        // Ancestor: if `~/Projects` is trusted, `~/Projects/foo` is trusted.
        for (stored, flag) in &store.directories {
            if !*flag {
                continue;
            }
            let stored_exp = expand_user_path(Path::new(stored));
            let stored_can = stored_exp.canonicalize().unwrap_or(stored_exp);
            let key_path = PathBuf::from(key);
            if key_path.starts_with(&stored_can) {
                return true;
            }
        }
        false
    }

    /// Whether project-local hook commands may load.
    pub fn project_hooks_allowed<P: AppPaths>(paths: &P, cwd: &Path) -> Result<bool> {
        let store = Self::load(paths)?;
        let key = Self::storage_key(cwd);
        if let Some(decision) = store.directories.get(&key) {
            return Ok(*decision);
        }
        if Self::is_trusted_in_store(&store, &key) {
            return Ok(true);
        }
        Ok(matches!(store.default_project_trust, DefaultProjectTrust::Always))
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn expand_user_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    } else if s == "~" {
        if let Some(home) = home_dir() {
            return home;
        }
    } else if let Some(rest) = s.strip_prefix("$HOME/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    } else if s == "$HOME"
        && let Some(home) = home_dir()
    {
        return home;
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Paths;

    fn test_paths(label: &str) -> Paths {
        let root = std::env::temp_dir().join(format!(
            "elph-trust-test-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = root.join("config");
        let data = root.join("data");
        let project = root.join("project");
        std::fs::create_dir_all(&config).expect("config");
        std::fs::create_dir_all(&data).expect("data");
        std::fs::create_dir_all(&project).expect("project");
        Paths::from_dirs(config, data, project)
    }

    #[test]
    fn default_serializes_empty_directories() {
        let json = serde_json::to_string(&TrustStore::default()).expect("serialize");
        assert!(json.contains("\"directories\":{}"));
    }

    #[test]
    fn trust_directory_writes_config_trust_json() {
        let paths = test_paths("write");
        let cwd = paths.project_dir().clone();
        let key = TrustStore::trust_directory(&paths, &cwd).expect("trust");
        assert!(paths.trust_path().exists());
        let loaded = TrustStore::load(&paths).expect("load");
        assert_eq!(loaded.directories.get(&key), Some(&true));
        assert!(TrustStore::is_trusted(&paths, &cwd).expect("is_trusted"));
    }

    #[test]
    fn untrust_directory_writes_false_and_overrides_trusted_ancestor() {
        let paths = test_paths("untrust");
        let child = paths.project_dir().join("child");
        std::fs::create_dir_all(&child).expect("child");
        TrustStore::trust_directory(&paths, paths.project_dir()).expect("trust parent");
        assert!(TrustStore::is_trusted(&paths, &child).expect("inherited trust"));

        let key = TrustStore::untrust_directory(&paths, &child).expect("untrust child");
        let loaded = TrustStore::load(&paths).expect("load");
        assert_eq!(loaded.directories.get(&key), Some(&false));
        assert!(!TrustStore::is_trusted(&paths, &child).expect("explicit untrust"));
        let mut loaded = loaded;
        loaded.default_project_trust = DefaultProjectTrust::Always;
        loaded.save(&paths).expect("save default");
        assert!(!TrustStore::project_hooks_allowed(&paths, &child).expect("hooks denied"));
    }

    #[test]
    fn untrust_directory_rejects_untrusted_project() {
        let paths = test_paths("untrust-reject");
        let error = TrustStore::untrust_directory(&paths, paths.project_dir()).expect_err("untrust should fail");
        assert!(error.to_string().contains("project is not trusted"));
    }
}
