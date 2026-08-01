use crate::utils::path::AppPaths;
use anyhow::Result;
use elph_agent::write_json_file;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;

/// Structured changelog written to `APP_DATA/CHANGELOG.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangelogFile {
    #[serde(default)]
    pub entries: Vec<ChangelogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangelogEntry {
    pub version: String,
    pub date: String,
    pub notes: Vec<String>,
}

pub struct ChangelogScaffold;

impl ChangelogScaffold {
    pub fn ensure<P: AppPaths>(paths: &P) -> Result<()> {
        ensure_md(paths)?;
        ensure_json(paths)?;
        Ok(())
    }
}

fn ensure_md<P: AppPaths>(paths: &P) -> Result<()> {
    let path = paths.changelog_md_path();
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(&path)?;
    writeln!(file, "# Changelog")?;
    writeln!(file)?;
    writeln!(file, "Release notes for Elph. Structured data also lives in `CHANGELOG.json`.")?;
    Ok(())
}

fn ensure_json<P: AppPaths>(paths: &P) -> Result<()> {
    let path = paths.changelog_json_path();
    if path.exists() {
        return Ok(());
    }
    write_json_file(&path, &ChangelogFile::default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_json_is_empty_entries() {
        let json = serde_json::to_string(&ChangelogFile::default()).expect("serialize");
        assert_eq!(json, r#"{"entries":[]}"#);
    }
}
