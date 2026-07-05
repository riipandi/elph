use std::collections::BTreeMap;

use crate::layout::files::write_json_file;
use crate::utils::path::AppPaths;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BundledManifest {
    pub version: String,
    #[serde(default)]
    pub checksums: BTreeMap<String, String>,
}

impl BundledManifest {
    pub fn defaults(app_id: &str, app_version: &str) -> Self {
        Self {
            version: format!("{app_id}-{app_version}"),
            checksums: BTreeMap::new(),
        }
    }

    pub fn ensure<P: AppPaths>(paths: &P, app_id: &str, app_version: &str) -> Result<()> {
        let path = paths.bundled_manifest_path();
        if path.exists() {
            return Ok(());
        }

        write_json_file(&path, &Self::defaults(app_id, app_version))?;
        Ok(())
    }
}
