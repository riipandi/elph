//! Extension discovery (Pi-compatible layout, Elph paths).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use super::types::{ExtensionManifest, ExtensionsSettings};

/// Global extensions: `~/.elph/extensions/`
pub fn global_extensions_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("extensions")
}

/// Project-local extensions: `<project>/.elph/extensions/`
pub fn project_extensions_dir(project_elph_dir: &Path) -> PathBuf {
    project_elph_dir.join("extensions")
}

/// Collect extension roots in load order: global, project-local, then extra paths.
pub fn extension_roots(
    config_dir: &Path,
    project_elph_dir: &Path,
    settings: &ExtensionsSettings,
    include_project: bool,
) -> Vec<PathBuf> {
    let mut roots = vec![global_extensions_dir(config_dir)];
    if include_project {
        roots.push(project_extensions_dir(project_elph_dir));
    }
    roots.extend(settings.extra_paths.clone());
    roots
}

/// Discover extension manifests under the given roots.
pub fn discover_manifests(roots: &[PathBuf]) -> Result<Vec<(PathBuf, ExtensionManifest)>> {
    let mut found = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(root)
            .min_depth(1)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == "extension.toml") {
                let manifest = load_manifest(path)?;
                let dir = path.parent().context("extension.toml parent")?.to_path_buf();
                found.push((dir, manifest));
            }
        }
    }
    found.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    Ok(found)
}

pub fn load_manifest(path: &Path) -> Result<ExtensionManifest> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}
