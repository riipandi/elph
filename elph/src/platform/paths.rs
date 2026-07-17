use std::path::{Path, PathBuf};

pub use crate::utils::path::AppPaths;
use crate::utils::path::{PathResolver, ResolvedPaths};
use crate::utils::project_key;
use anyhow::Result;

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
        self.project_elph_dir().join("store.db")
    }

    pub fn project_gitignore_path(&self) -> PathBuf {
        self.project_elph_dir().join(".gitignore")
    }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.config_dir().clone(), self.data_dir().clone()];
        dirs.extend(self.standard_required_dirs());
        dirs.push(self.global_extensions_dir());
        dirs.push(self.project_elph_dir());
        dirs.push(self.project_extensions_dir());
        if let Ok(layout) = self.project_layout_dirs() {
            dirs.extend(layout);
        }
        dirs
    }

    /// Stable `{hash}_{folder_name}` key for the current project directory.
    pub fn project_key(&self) -> Result<String> {
        project_key::from_path(self.project_dir())
    }

    /// `~/.elph/projects/<key>/`
    pub fn project_data_dir(&self) -> Result<PathBuf> {
        Ok(self.projects_dir().join(self.project_key()?))
    }

    /// `~/.elph/sessions/<key>/`
    #[allow(dead_code)]
    pub fn project_sessions_dir(&self) -> Result<PathBuf> {
        Ok(self.sessions_dir().join(self.project_key()?))
    }

    /// Per-project runtime directories (mcps, terminals, agent-tools).
    pub fn project_layout_dirs(&self) -> Result<Vec<PathBuf>> {
        let base = self.project_data_dir()?;
        Ok(vec![base.join("mcps"), base.join("terminals"), base.join("agent-tools")])
    }

    /// Resolve layout dirs for an arbitrary project path (e.g. session resume).
    #[allow(dead_code)]
    /// `~/.elph/extensions/`
    pub fn global_extensions_dir(&self) -> PathBuf {
        elph_agent::global_extensions_dir(self.config_dir())
    }

    /// `<project>/.elph/extensions/`
    pub fn project_extensions_dir(&self) -> PathBuf {
        elph_agent::project_extensions_dir(&self.project_elph_dir())
    }

    /// Project MCP override: `<project>/.elph/mcp.json` (merged over home `mcp.json`).
    pub fn project_mcp_config_path(&self) -> PathBuf {
        self.project_elph_dir().join("mcp.json")
    }

    /// Project settings override: `<project>/.elph/settings.json` (merged over home settings).
    pub fn project_settings_path(&self) -> PathBuf {
        self.project_elph_dir().join("settings.json")
    }

    pub fn project_layout_dirs_for(&self, project_path: &Path) -> Result<Vec<PathBuf>> {
        let key = project_key::from_path(project_path)?;
        let base = self.projects_dir().join(key);
        Ok(vec![base.join("mcps"), base.join("terminals"), base.join("agent-tools")])
    }

    /// Session storage root for a project path.
    #[allow(dead_code)]
    pub fn project_sessions_dir_for(&self, project_path: &Path) -> Result<PathBuf> {
        Ok(self.sessions_dir().join(project_key::from_path(project_path)?))
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
        assert_eq!(paths.memory_db_path(), project.join(".elph/store.db"));
        assert_eq!(paths.project_gitignore_path(), project.join(".elph/.gitignore"));
        assert_eq!(paths.project_settings_path(), project.join(".elph/settings.json"));
        assert_eq!(paths.project_mcp_config_path(), project.join(".elph/mcp.json"));
        assert_eq!(paths.bundled_manifest_path(), config.join("bundled/manifest.json"));
        assert_eq!(paths.models_dir(), data.join("models"));
        // 15 standard + config/data + global/project extensions + project_elph + 3 layout dirs
        assert_eq!(paths.required_dirs().len(), 23);
    }
}
