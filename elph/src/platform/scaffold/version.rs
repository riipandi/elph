use crate::utils::path::AppPaths;
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use elph_agent::write_json_file;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VersionFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_providers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_version: Option<String>,
    pub version: String,
}

impl VersionFile {
    pub fn defaults(app_version: &str) -> Self {
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true);
        Self {
            last_sync_providers: None,
            release_checked_at: Some(now),
            stable_version: None,
            version: app_version.to_string(),
        }
    }

    pub fn ensure<P: AppPaths>(paths: &P, app_version: &str) -> Result<()> {
        let path = paths.version_path();
        if path.exists() {
            return Ok(());
        }

        write_json_file(&path, &Self::defaults(app_version))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_release_checked_at_is_rfc3339_utc() {
        let file = VersionFile::defaults("0.0.1");
        let stamp = file.release_checked_at.expect("release_checked_at");
        assert!(stamp.ends_with('Z'));
        assert!(stamp.contains('T'));
    }
}
