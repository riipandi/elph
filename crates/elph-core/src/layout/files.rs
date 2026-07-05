use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

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
pub fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("failed to write {}", path.display()))?;
        file.write_all(contents)?;
        return Ok(());
    }

    #[cfg(not(unix))]
    {
        fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}
