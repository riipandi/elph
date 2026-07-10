use std::path::PathBuf;

use anyhow::Result;
pub use elph_core::utils::path::AppPaths;
use elph_core::utils::path::PathResolver;

pub const RESOLVER: PathResolver = PathResolver {
    home_env: "ECLAW_HOME",
    data_env: "ECLAW_DATA_DIR",
    project_env: "ECLAW_PROJECT_DIR",
    config_dir_name: ".eclaw",
    data_dir_name: "eclaw",
};

/// Eclaw config and data paths (`~/.eclaw`, `~/.local/share/eclaw`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self> {
        let resolved = RESOLVER.resolve()?;
        Ok(Self {
            config_dir: resolved.config_dir,
            data_dir: resolved.data_dir,
        })
    }

    #[allow(dead_code)]
    pub fn from_dirs(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self { config_dir, data_dir }
    }

    pub fn memory_db_path(&self) -> PathBuf {
        self.data_dir.join("store.db")
    }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.config_dir.clone(), self.data_dir.clone()];
        dirs.extend(self.standard_required_dirs());
        dirs
    }
}

impl AppPaths for Paths {
    fn config_dir(&self) -> &PathBuf {
        &self.config_dir
    }

    fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}

#[cfg(test)]
mod tests {
    use super::{AppPaths, *};

    #[test]
    fn builds_expected_file_paths() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = tmp.path().join("config");
        let data = tmp.path().join("data");
        let paths = Paths::from_dirs(config.clone(), data.clone());

        assert_eq!(paths.metadata_db_path(), data.join("metadata.db"));
        assert_eq!(paths.memory_db_path(), data.join("store.db"));
        assert_eq!(paths.bundled_manifest_path(), config.join("bundled/manifest.json"));
        assert_eq!(paths.required_dirs().len(), 17);
    }
}
