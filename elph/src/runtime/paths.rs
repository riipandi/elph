use std::path::PathBuf;

use anyhow::Result;
pub use elph_core::utils::path::AppPaths;
use elph_core::utils::path::{PathResolver, ResolvedPaths};

const PROJECT_DIR_NAME: &str = ".elph";

pub const RESOLVER: PathResolver = PathResolver {
    home_env: "ELPH_HOME",
    data_env: "ELPH_DATA_DIR",
    project_env: "ELPH_PROJECT_DIR",
    config_dir_name: ".elph",
    data_dir_name: "elph",
};

/// Elph-specific config, data, and project paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    inner: ResolvedPaths,
}

impl Paths {
    pub fn resolve() -> Result<Self> {
        Ok(Self {
            inner: RESOLVER.resolve()?,
        })
    }

    #[allow(dead_code)]
    pub fn from_dirs(config_dir: PathBuf, data_dir: PathBuf, project_dir: PathBuf) -> Self {
        Self {
            inner: ResolvedPaths::from_dirs(config_dir, data_dir, project_dir),
        }
    }

    #[allow(dead_code)]
    pub fn project_dir(&self) -> &PathBuf {
        &self.inner.project_dir
    }

    pub fn project_elph_dir(&self) -> PathBuf {
        self.inner.project_dir.join(PROJECT_DIR_NAME)
    }

    /// Project-local floppy store (Turso DB).
    pub fn memory_db_path(&self) -> PathBuf {
        self.project_elph_dir().join("memory.db")
    }

    pub fn project_gitignore_path(&self) -> PathBuf {
        self.project_elph_dir().join(".gitignore")
    }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.config_dir().clone(), self.data_dir().clone()];
        dirs.extend(self.standard_required_dirs());
        dirs.push(self.project_elph_dir());
        dirs
    }
}

impl AppPaths for Paths {
    fn config_dir(&self) -> &PathBuf {
        &self.inner.config_dir
    }

    fn data_dir(&self) -> &PathBuf {
        &self.inner.data_dir
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
        let project = tmp.path().join("repo");
        let paths = Paths::from_dirs(config.clone(), data.clone(), project.clone());

        assert_eq!(paths.metadata_db_path(), data.join("metadata.db"));
        assert_eq!(paths.memory_db_path(), project.join(".elph/memory.db"));
        assert_eq!(paths.project_gitignore_path(), project.join(".elph/.gitignore"));
        assert_eq!(paths.bundled_manifest_path(), config.join("bundled/manifest.json"));
        assert_eq!(paths.models_dir(), data.join("models"));
        assert_eq!(paths.required_dirs().len(), 17);
    }
}
