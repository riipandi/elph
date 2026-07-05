mod progress;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

pub use progress::InitProgress;

/// Create every directory in `dirs`, including parents.
pub fn ensure_dirs(dirs: &[PathBuf]) -> Result<()> {
    for dir in dirs {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// Write a pretty-printed JSON file with mode `0600` on Unix.
pub fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut payload = serde_json::to_string_pretty(value)?;
    payload.push('\n');

    write_private_file(path, payload.as_bytes())
}

/// Write a private file with mode `0600` on Unix.
pub fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
        file.write_all(contents)?;
        return Ok(());
    }

    #[cfg(not(unix))]
    {
        fs::write(path, contents)?;
        Ok(())
    }
}

/// Write a file only when it does not already exist.
pub fn write_file_if_missing(path: &Path, contents: &[u8]) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, contents)?;
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
}
