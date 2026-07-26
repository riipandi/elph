use crate::utils::path::AppPaths;
use anyhow::Result;
use elph_agent::write_json_file;

pub struct TrustStore;

impl TrustStore {
    pub fn ensure<P: AppPaths>(paths: &P) -> Result<()> {
        let path = paths.trust_path();
        if path.exists() {
            return Ok(());
        }

        write_json_file(&path, &serde_json::json!({}))?;
        Ok(())
    }
}
