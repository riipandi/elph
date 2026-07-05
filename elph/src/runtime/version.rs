use elph_agent::utils::utc_rfc3339_now;
use elph_agent::write_json_file;
use serde::{Deserialize, Serialize};

use super::paths::Paths;
use anyhow::Result;

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
        let now = utc_rfc3339_now();
        Self {
            last_sync_providers: None,
            release_checked_at: Some(now),
            stable_version: None,
            version: app_version.to_string(),
        }
    }

    pub fn ensure(paths: &Paths, app_version: &str) -> Result<()> {
        let path = paths.version_path();
        if path.exists() {
            return Ok(());
        }

        write_json_file(&path, &Self::defaults(app_version))?;
        Ok(())
    }
}
