//! MCP configuration persistence and project cache paths.

use std::path::Path;

use anyhow::{Context, Result};
use elph_agent::{McpConfig, McpServerConfig, write_json_file};
use elph_core::utils::path::AppPaths;

use super::paths::Paths;

pub fn load_config(paths: &Paths) -> Result<McpConfig> {
    let path = paths.mcp_config_path();
    if !path.exists() {
        return Ok(McpConfig::default());
    }
    let content = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("parse {}", path.display()))
}

pub fn save_config(paths: &Paths, config: &McpConfig) -> Result<()> {
    write_json_file(&paths.mcp_config_path(), config).context("write mcp.json")
}

pub fn upsert_server(paths: &Paths, name: &str, server: McpServerConfig) -> Result<()> {
    let mut config = load_config(paths)?;
    config.servers.insert(name.to_string(), server);
    save_config(paths, &config)
}

pub fn remove_server(paths: &Paths, name: &str) -> Result<bool> {
    let mut config = load_config(paths)?;
    let removed = config.servers.remove(name).is_some();
    if removed {
        save_config(paths, &config)?;
    }
    Ok(removed)
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
        return serde_json::from_str(&content).context("parse MCP config file");
    }
    serde_json::from_str(raw).context("parse MCP config JSON")
}
