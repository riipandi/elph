use serde_json::Value;
use std::path::{Path, PathBuf};

use super::types::Checkpoint;

pub fn decode_write_value(raw: &Value) -> Value {
    if let Some(text) = raw.as_str()
        && let Ok(parsed) = serde_json::from_str::<Value>(text)
    {
        return parsed;
    }
    raw.clone()
}

pub fn filter_bind_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

pub fn compare_channel_versions(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<i64>(), b.parse::<i64>()) {
        (Ok(a_num), Ok(b_num)) => a_num.cmp(&b_num),
        _ => a.cmp(b),
    }
}

pub fn max_channel_version(versions: Vec<String>) -> String {
    versions
        .into_iter()
        .max_by(|a, b| compare_channel_versions(a.as_str(), b.as_str()))
        .unwrap_or_else(|| "1".to_string())
}

#[cfg(unix)]
pub fn secure_dir(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
pub fn secure_dir(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub fn secure_file(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn secure_file(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

/// Default checkpoint DB path (`~/.owly/owly.sqlite`), mirroring OpenWiki's layout.
pub fn default_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".owly").join("owly.sqlite")
}

/// Copy a checkpoint for persistence (shallow channel map copy).
pub fn copy_checkpoint(checkpoint: &Checkpoint) -> Checkpoint {
    Checkpoint {
        v: checkpoint.v,
        id: checkpoint.id.clone(),
        ts: checkpoint.ts.clone(),
        channel_values: checkpoint.channel_values.clone(),
        channel_versions: checkpoint.channel_versions.clone(),
        versions_seen: checkpoint.versions_seen.clone(),
    }
}
