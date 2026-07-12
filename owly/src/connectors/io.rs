//! Connector filesystem I/O under `~/.owly/connectors/`.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::types::{ConnectorId, ConnectorState};

pub fn connectors_root() -> PathBuf {
    crate::mode::owly_home_dir().join("connectors")
}

pub fn connector_home(id: ConnectorId) -> PathBuf {
    connectors_root().join(id.as_str())
}

pub fn config_path(id: ConnectorId) -> PathBuf {
    connector_home(id).join("config.json")
}

pub fn state_path(id: ConnectorId) -> PathBuf {
    connector_home(id).join("state.json")
}

pub fn raw_dir(id: ConnectorId) -> PathBuf {
    connector_home(id).join("raw")
}

pub fn ensure_connector_home(id: ConnectorId) -> Result<()> {
    let home = connector_home(id);
    std::fs::create_dir_all(&home).with_context(|| format!("create {}", home.display()))?;
    std::fs::create_dir_all(raw_dir(id))?;
    crate::credentials::secure_env_dir()?;
    Ok(())
}

pub fn read_connector_config<T: serde::de::DeserializeOwned + Default>(id: ConnectorId) -> Result<T> {
    ensure_connector_home(id)?;
    let path = config_path(id);
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).or(Ok(T::default()))
}

pub fn write_connector_config<T: serde::Serialize>(id: ConnectorId, config: &T) -> Result<PathBuf> {
    ensure_connector_home(id)?;
    let path = config_path(id);
    write_private_json(&path, config)?;
    Ok(path)
}

pub fn read_connector_state(id: ConnectorId) -> Result<ConnectorState> {
    ensure_connector_home(id)?;
    let path = state_path(id);
    if !path.exists() {
        return Ok(ConnectorState {
            version: 1,
            ..ConnectorState::default()
        });
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn write_connector_state(id: ConnectorId, state: &ConnectorState) -> Result<()> {
    ensure_connector_home(id)?;
    write_private_json(&state_path(id), state)
}

pub fn write_raw_json(id: ConnectorId, run_id: &str, filename: &str, value: &impl serde::Serialize) -> Result<String> {
    ensure_connector_home(id)?;
    let dir = raw_dir(id).join(run_id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(filename);
    write_private_json(&path, value)?;
    Ok(path.display().to_string())
}

pub fn create_run_id() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S%.3fZ").to_string()
}

pub fn update_state_with_run(state: ConnectorState, record: super::types::ConnectorRunRecord) -> ConnectorState {
    let mut runs = state.runs;
    runs.insert(0, record.clone());
    runs.truncate(20);
    ConnectorState {
        last_run_at: Some(record.at),
        runs,
        ..state
    }
}

fn write_private_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = format!("{}\n", serde_json::to_string_pretty(value)?);
    std::fs::write(path, &body).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
