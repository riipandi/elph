use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

/// Create every directory in `dirs`, including parents.
pub fn ensure_dirs(dirs: &[PathBuf]) -> Result<()> {
    for dir in dirs {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    }
    Ok(())
}

/// Write a pretty-printed JSON file with mode `0600` on Unix.
pub fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut payload = serde_json::to_string_pretty(value).context("failed to serialize json")?;
    payload.push('\n');
    write_private_file(path, payload.as_bytes())
}

/// Write a private file with mode `0600` on Unix.
///
/// Overwrites the destination when it already exists (create + truncate).
pub fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("failed to write {}", path.display()))?;
        file.write_all(contents)?;
        Ok(())
    }

    #[cfg(not(unix))]
    {
        fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

/// Write a file only when it does not already exist.
pub fn write_file_if_missing(path: &Path, contents: &[u8]) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    }

    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_file_if_missing_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("marker.txt");

        write_file_if_missing(&path, b"first").expect("first write");
        write_file_if_missing(&path, b"second").expect("second write");

        assert_eq!(fs::read_to_string(path).expect("read marker"), "first");
    }

    #[test]
    fn ensure_dirs_creates_all() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dirs: Vec<PathBuf> = vec![tmp.path().join("a"), tmp.path().join("b/c")];
        ensure_dirs(&dirs).expect("ensure_dirs");
        assert!(tmp.path().join("a").exists());
        assert!(tmp.path().join("b/c").exists());
    }

    #[test]
    fn write_private_file_creates_new() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("private.txt");
        write_private_file(&path, b"secret").expect("write_private_file");
        assert_eq!(fs::read_to_string(&path).expect("read"), "secret");
    }

    #[test]
    fn write_private_file_overwrites_existing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("private.txt");
        write_private_file(&path, b"first").expect("first write");
        write_private_file(&path, b"second").expect("second write");
        assert_eq!(fs::read_to_string(&path).expect("read"), "second");
    }

    #[test]
    fn write_json_file_overwrites_existing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        write_json_file(&path, &serde_json::json!({"a": 1})).expect("first");
        write_json_file(&path, &serde_json::json!({"a": 2})).expect("second");
        let parsed: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(parsed["a"], 2);
    }
}

#[test]
fn write_json_file_produces_valid_json() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("test.json");
    let data = serde_json::json!({"name": "test", "count": 42});
    write_json_file(&path, &data).expect("write");
    let content = fs::read_to_string(&path).expect("read");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("parse");
    assert_eq!(parsed, data);
}
