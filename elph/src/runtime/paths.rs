use std::path::PathBuf;

use anyhow::Result;
use elph_agent::utils::{PathResolver, ResolvedPaths};

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

    pub fn config_dir(&self) -> &PathBuf {
        &self.inner.config_dir
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.inner.data_dir
    }

    #[allow(dead_code)]
    pub fn project_dir(&self) -> &PathBuf {
        &self.inner.project_dir
    }

    pub fn settings_path(&self) -> PathBuf {
        self.inner.config_dir.join("settings.json")
    }

    pub fn trust_path(&self) -> PathBuf {
        self.inner.config_dir.join("trust.json")
    }

    pub fn bundled_dir(&self) -> PathBuf {
        self.inner.config_dir.join("bundled")
    }

    pub fn bundled_manifest_path(&self) -> PathBuf {
        self.bundled_dir().join("manifest.json")
    }

    pub fn prompts_dir(&self) -> PathBuf {
        self.inner.config_dir.join("prompts")
    }

    pub fn providers_dir(&self) -> PathBuf {
        self.inner.config_dir.join("providers")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.inner.config_dir.join("sessions")
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.inner.config_dir.join("skills")
    }

    pub fn worktrees_dir(&self) -> PathBuf {
        self.inner.config_dir.join("worktrees")
    }

    pub fn attachments_dir(&self) -> PathBuf {
        self.inner.data_dir.join("attachments")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.inner.data_dir.join("downloads")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.inner.data_dir.join("logs")
    }

    pub fn vendor_dir(&self) -> PathBuf {
        self.inner.data_dir.join("vendor")
    }

    pub fn metadata_db_path(&self) -> PathBuf {
        self.inner.data_dir.join("metadata.db")
    }

    pub fn project_elph_dir(&self) -> PathBuf {
        self.inner.project_dir.join(PROJECT_DIR_NAME)
    }

    pub fn memory_db_path(&self) -> PathBuf {
        self.project_elph_dir().join("memory.db")
    }

    pub fn project_gitignore_path(&self) -> PathBuf {
        self.project_elph_dir().join(".gitignore")
    }

    pub fn version_path(&self) -> PathBuf {
        self.inner.data_dir.join("version.json")
    }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.bundled_dir().join("agents"),
            self.bundled_dir().join("personas"),
            self.bundled_dir().join("skills"),
            self.bundled_dir().join("user-guide"),
            self.prompts_dir(),
            self.providers_dir(),
            self.sessions_dir(),
            self.skills_dir(),
            self.worktrees_dir(),
            self.attachments_dir(),
            self.downloads_dir(),
            self.logs_dir(),
            self.vendor_dir(),
            self.project_elph_dir(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(paths.required_dirs().len(), 14);
    }
}
