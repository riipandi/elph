//! Stable project/session directory keys: `{hash}_{folder_name}`.

use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rustc_hash::FxBuildHasher;

const CHAT_HISTORY_FILE: &str = "chat_history.jsonl";

/// Canonical chat history filename under a session directory.
pub fn chat_history_filename() -> &'static str {
    CHAT_HISTORY_FILE
}

/// Build a stable directory key from a project path.
///
/// Format: `{16-hex-hash}_{sanitized_folder_name}`
///
/// The hash is derived from the canonical absolute path. The folder name is the
/// basename of the project directory (not the full path), with unsafe characters
/// replaced by underscores.
pub fn from_path(path: &Path) -> Result<String> {
    let canonical = canonicalize_project_path(path)?;
    let path_str = canonical.to_string_lossy();
    let hash = format!("{:016x}", FxBuildHasher.hash_one(path_str.as_ref()));
    let folder_name = sanitize_folder_name(basename(&canonical));
    Ok(format!("{hash}_{folder_name}"))
}

/// Sanitize a folder basename for use in directory names.
pub fn sanitize_folder_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_underscore = false;
    for ch in name.chars() {
        let safe = ch.is_ascii_alphanumeric() || ch == '-' || ch == '_';
        if safe {
            out.push(ch);
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "project".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Extract the sanitized folder-name suffix from a project key, if present.
pub fn folder_name_from_key(key: &str) -> Option<&str> {
    let (_, suffix) = key.split_once('_')?;
    if suffix.is_empty() { None } else { Some(suffix) }
}

fn canonicalize_project_path(path: &Path) -> Result<PathBuf> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return Ok(canonical);
    }

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(std::fs::canonicalize)
        .transpose()?;
    let file_name = path.file_name().context("project path must have a file name")?;
    Ok(parent.map(|p| p.join(file_name)).unwrap_or_else(|| path.to_path_buf()))
}

fn basename(path: &Path) -> &str {
    path.file_name().and_then(|s| s.to_str()).unwrap_or("project")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_unsafe_chars() {
        assert_eq!(sanitize_folder_name("my-project"), "my-project");
        assert_eq!(sanitize_folder_name("foo bar"), "foo_bar");
        assert_eq!(sanitize_folder_name("a..b"), "a_b");
        assert_eq!(sanitize_folder_name("___"), "project");
    }

    #[test]
    fn key_is_stable_for_same_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let a = from_path(dir.path()).expect("key");
        let b = from_path(dir.path()).expect("key");
        assert_eq!(a, b);
        assert!(a.contains('_'));
        assert!(folder_name_from_key(&a).is_some());
    }

    #[test]
    fn folder_name_uses_basename_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested = dir.path().join("my_repo");
        std::fs::create_dir_all(&nested).expect("mkdir");
        let key = from_path(&nested).expect("key");
        assert!(key.ends_with("_my_repo"));
    }
}
