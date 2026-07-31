use crate::utils::path::AppPaths;
use anyhow::Result;
use elph_agent::write_json_file;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Trusted workspace directories (`CONFIG_DIR/trust.json`).
///
/// Paths may use `~`, `$HOME`, or absolute forms. Values are trust flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustStore {
    #[serde(default)]
    pub directories: BTreeMap<String, bool>,
}

impl TrustStore {
    pub fn ensure<P: AppPaths>(paths: &P) -> Result<()> {
        let path = paths.trust_path();
        if path.exists() {
            return Ok(());
        }

        write_json_file(&path, &Self::default())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_serializes_empty_directories() {
        let json = serde_json::to_string(&TrustStore::default()).expect("serialize");
        assert_eq!(json, r#"{"directories":{}}"#);
    }
}
