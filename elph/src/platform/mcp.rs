//! MCP configuration persistence, home + project merge, and project cache paths.
//!
//! # Layers
//!
//! | Layer   | Path                         | Role                                      |
//! |---------|------------------------------|-------------------------------------------|
//! | Home    | `~/.elph/mcp.json`           | Global servers (CLI default write target) |
//! | Project | `<project>/.elph/mcp.json`   | Override / add servers for this repo      |
//!
//! Runtime load merges **home ← project** (project wins on same server name).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use elph_agent::{
    McpConfig, McpServerConfig, parse_and_validate_mcp_config, parse_and_validate_server_config_json, write_json_file,
};
use elph_core::utils::path::AppPaths;

use super::paths::Paths;

/// Which config file to read/write for CLI mutations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum McpConfigScope {
    /// `~/.elph/mcp.json` (default for `mcp add` / `remove`).
    #[default]
    Home,
    /// `<project>/.elph/mcp.json`.
    Project,
}

impl McpConfigScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Project => "project",
        }
    }
}

/// Path for a single layer.
pub fn config_path(paths: &Paths, scope: McpConfigScope) -> PathBuf {
    match scope {
        McpConfigScope::Home => paths.mcp_config_path(),
        McpConfigScope::Project => paths.project_mcp_config_path(),
    }
}

/// Load one layer (missing file → empty config).
pub fn load_layer(paths: &Paths, scope: McpConfigScope) -> Result<McpConfig> {
    load_file(&config_path(paths, scope))
}

/// Save one layer (validates first). Creates parent dirs for project scope.
pub fn save_layer(paths: &Paths, scope: McpConfigScope, config: &McpConfig) -> Result<()> {
    elph_agent::validate_mcp_config(config).map_err(|e| anyhow::anyhow!("{e}"))?;
    let path = config_path(paths, scope);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    write_json_file(&path, config).with_context(|| format!("write {}", path.display()))
}

fn load_file(path: &Path) -> Result<McpConfig> {
    if !path.exists() {
        return Ok(McpConfig::default());
    }
    let content = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse_and_validate_mcp_config(&content).with_context(|| format!("validate {}", path.display()))
}

/// Merged home + project config (project wins). Used by agent runtime and doctor probes.
pub fn load_config(paths: &Paths) -> Result<McpConfig> {
    let home = load_layer(paths, McpConfigScope::Home)?;
    let project = load_layer(paths, McpConfigScope::Project)?;
    Ok(home.merge_with(&project))
}

/// Async load + validate each layer, then merge.
pub async fn load_config_async(paths: &Paths) -> Result<McpConfig> {
    let home = load_layer_async(paths, McpConfigScope::Home).await?;
    let project = load_layer_async(paths, McpConfigScope::Project).await?;
    Ok(home.merge_with(&project))
}

async fn load_layer_async(paths: &Paths, scope: McpConfigScope) -> Result<McpConfig> {
    let path = config_path(paths, scope);
    if !path.exists() {
        return Ok(McpConfig::default());
    }
    let content = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    elph_agent::parse_and_validate_mcp_config_async(content)
        .await
        .with_context(|| format!("validate {}", path.display()))
}

/// Save merged view is not supported — write a specific layer instead.
pub fn save_config(paths: &Paths, config: &McpConfig) -> Result<()> {
    save_layer(paths, McpConfigScope::Home, config)
}

pub fn upsert_server(paths: &Paths, name: &str, server: McpServerConfig) -> Result<()> {
    upsert_server_in(paths, McpConfigScope::Home, name, server)
}

pub fn upsert_server_in(paths: &Paths, scope: McpConfigScope, name: &str, server: McpServerConfig) -> Result<()> {
    let mut config = load_layer(paths, scope)?;
    config.servers.insert(name.to_string(), server);
    save_layer(paths, scope, &config)
}

pub fn remove_server(paths: &Paths, name: &str) -> Result<bool> {
    remove_server_in(paths, McpConfigScope::Home, name)
}

pub fn remove_server_in(paths: &Paths, scope: McpConfigScope, name: &str) -> Result<bool> {
    let mut config = load_layer(paths, scope)?;
    let removed = config.servers.remove(name).is_some();
    if removed {
        save_layer(paths, scope, &config)?;
    }
    Ok(removed)
}

/// Where a server name is defined after merge (for CLI listing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpServerSource {
    Home,
    Project,
    /// Same name exists in both layers; effective definition is from project.
    ProjectOverHome,
}

/// Resolve which layer(s) define each server name.
pub fn server_sources(paths: &Paths) -> Result<std::collections::BTreeMap<String, McpServerSource>> {
    let home = load_layer(paths, McpConfigScope::Home)?;
    let project = load_layer(paths, McpConfigScope::Project)?;
    let mut out = std::collections::BTreeMap::new();
    for name in home.servers.keys() {
        out.insert(name.clone(), McpServerSource::Home);
    }
    for name in project.servers.keys() {
        if out.contains_key(name) {
            out.insert(name.clone(), McpServerSource::ProjectOverHome);
        } else {
            out.insert(name.clone(), McpServerSource::Project);
        }
    }
    Ok(out)
}

/// Project-local MCP cache directory: `~/.elph/projects/<key>/mcps/`.
pub fn project_mcp_cache_dir(paths: &Paths) -> Result<std::path::PathBuf> {
    Ok(paths.project_data_dir()?.join("mcps"))
}

pub fn ensure_project_mcp_cache(paths: &Paths) -> Result<std::path::PathBuf> {
    let dir = project_mcp_cache_dir(paths)?;
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

pub fn parse_server_config(raw: &str) -> Result<McpServerConfig> {
    if Path::new(raw).exists() {
        let content = std::fs::read_to_string(raw).with_context(|| format!("read {raw}"))?;
        return parse_and_validate_server_config_json(&content);
    }
    parse_and_validate_server_config_json(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_core::utils::path::AppPaths;
    use tempfile::tempdir;

    fn test_paths(tmp: &tempfile::TempDir) -> Paths {
        let config = tmp.path().join("config");
        let data = tmp.path().join("data");
        let project = tmp.path().join("repo");
        std::fs::create_dir_all(project.join(".elph")).unwrap();
        Paths::from_dirs(config, data, project)
    }

    #[test]
    fn merge_loads_project_over_home() {
        let tmp = tempdir().unwrap();
        let paths = test_paths(&tmp);

        let home = parse_and_validate_mcp_config(
            r#"{"servers":{"a":{"type":"stdio","command":"home"},"b":{"type":"stdio","command":"only-home"}}}"#,
        )
        .unwrap();
        save_layer(&paths, McpConfigScope::Home, &home).unwrap();

        let project = parse_and_validate_mcp_config(
            r#"{"servers":{"a":{"type":"stdio","command":"project"},"c":{"type":"stdio","command":"only-project"}}}"#,
        )
        .unwrap();
        save_layer(&paths, McpConfigScope::Project, &project).unwrap();

        let merged = load_config(&paths).unwrap();
        assert_eq!(merged.server_count(), 3);
        match merged.servers.get("a") {
            Some(McpServerConfig::Stdio(s)) => assert_eq!(s.command, "project"),
            other => panic!("{other:?}"),
        }
        assert!(merged.servers.contains_key("b"));
        assert!(merged.servers.contains_key("c"));

        let sources = server_sources(&paths).unwrap();
        assert_eq!(sources.get("a"), Some(&McpServerSource::ProjectOverHome));
        assert_eq!(sources.get("b"), Some(&McpServerSource::Home));
        assert_eq!(sources.get("c"), Some(&McpServerSource::Project));
    }

    #[test]
    fn upsert_project_does_not_write_home() {
        let tmp = tempdir().unwrap();
        let paths = test_paths(&tmp);
        upsert_server_in(
            &paths,
            McpConfigScope::Project,
            "local",
            McpServerConfig::stdio("npx", vec!["-y".into(), "x".into()]),
        )
        .unwrap();
        assert!(!paths.mcp_config_path().exists() || load_layer(&paths, McpConfigScope::Home).unwrap().is_empty());
        assert!(paths.project_mcp_config_path().exists());
        assert!(
            load_layer(&paths, McpConfigScope::Project)
                .unwrap()
                .servers
                .contains_key("local")
        );
    }

    #[test]
    fn project_path_is_under_elph() {
        let tmp = tempdir().unwrap();
        let paths = test_paths(&tmp);
        assert_eq!(paths.project_mcp_config_path(), paths.project_dir().join(".elph/mcp.json"));
    }
}
