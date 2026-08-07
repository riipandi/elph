use crate::utils::path::AppPaths;
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use elph_agent::write_json_file;
use serde::{Deserialize, Serialize};

/// Release / update metadata written to `APP_DATA/version.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VersionFile {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canary_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    /// Last successful models.dev / provider catalog sync (optional ops field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_providers: Option<String>,
}

impl VersionFile {
    pub fn defaults(app_version: &str) -> Self {
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true);
        Self {
            version: app_version.to_string(),
            stable_version: Some(app_version.to_string()),
            canary_version: Some(app_version.to_string()),
            last_checked_at: Some(now),
            last_sync_providers: None,
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
    fn defaults_last_checked_at_is_rfc3339_utc() {
        let file = VersionFile::defaults("0.0.1");
        let stamp = file.last_checked_at.expect("last_checked_at");
        assert!(stamp.ends_with('Z'));
        assert!(stamp.contains('T'));
        assert_eq!(file.version, "0.0.1");
        assert_eq!(file.stable_version.as_deref(), Some("0.0.1"));
        assert_eq!(file.canary_version.as_deref(), Some("0.0.1"));
    }
}
