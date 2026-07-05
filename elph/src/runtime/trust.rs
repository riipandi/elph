use elph_agent::write_json_file;

use super::paths::Paths;
use anyhow::Result;

pub struct TrustStore;

impl TrustStore {
    pub fn ensure(paths: &Paths) -> Result<()> {
        let path = paths.trust_path();
        if path.exists() {
            return Ok(());
        }

        write_json_file(&path, &serde_json::json!({}))?;
        Ok(())
    }
}
