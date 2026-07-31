use std::path::PathBuf;

pub use crate::utils::path::AppPaths;
use crate::utils::path::{PathResolver, ResolvedPaths};
use anyhow::Result;

const PROJECT_DIR_NAME: &str = ".elph";

pub const RESOLVER: PathResolver = PathResolver {
    home_env: "ELPH_HOME",
    data_env: "ELPH_DATA_DIR",
    project_env: "ELPH_PROJECT_DIR",
    config_dir_name: "elph",
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

    pub fn project_dir(&self) -> &PathBuf {
        &self.inner.project_dir
    }

    pub fn project_elph_dir(&self) -> PathBuf {
        self.inner.project_dir.join(PROJECT_DIR_NAME)
    }

    /// `PROJECT_DIR/.elph/plans/` — saved approved plan files.
    pub fn plans_dir(&self) -> PathBuf {
        self.project_elph_dir().join("plans")
    }

    /// Project-local floppy store (Turso DB).
    pub fn memory_db_path(&self) -> PathBuf {
        self.project_elph_dir().join("store.db")
    }

    /// Project-local transcript cache (Turso DB).
    pub fn transcript_db_path(&self) -> PathBuf {
        self.project_elph_dir().join("metadata.db")
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
        dirs
    }

    /// `CONFIG_DIR/extensions/`
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

    /// Derive session artifact dir from a Turso session's `db_path` + `id`
    /// (`{parent of metadata.db}/projects/{session_id}`).
    pub fn session_artifact_dir_from_db(db_path: &std::path::Path, session_id: &str) -> PathBuf {
        let data_dir = db_path.parent().unwrap_or_else(|| std::path::Path::new("."));
        data_dir.join("projects").join(session_id)
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
        assert_eq!(paths.transcript_db_path(), project.join(".elph/metadata.db"));
        assert_eq!(paths.project_gitignore_path(), project.join(".elph/.gitignore"));
        assert_eq!(paths.project_settings_path(), project.join(".elph/settings.json"));
        assert_eq!(paths.project_mcp_config_path(), project.join(".elph/mcp.json"));
        assert_eq!(paths.bundled_manifest_path(), config.join("bundled/manifest.json"));
        assert_eq!(paths.agents_dir(), config.join("agents"));
        assert_eq!(paths.hooks_dir(), config.join("hooks"));
        assert_eq!(paths.projects_dir(), data.join("projects"));
        assert_eq!(paths.host_mcp_cache_dir(), data.join("mcp_cache"));
        assert_eq!(paths.worktrees_dir(), data.join("worktrees"));
        assert_eq!(paths.sessions_dir(), data.join("sessions"));
        assert_eq!(paths.models_dir(), data.join("models"));
        assert_eq!(paths.session_artifact_dir("abc123"), data.join("projects").join("abc123"));
        assert_eq!(paths.session_mcp_cache_dir("abc123"), data.join("projects/abc123/mcp_cache"));
        assert_eq!(
            paths.mcp_tool_stderr_log_path("my server", "tool/name"),
            data.join("logs/mcp/my_server/tool_name.stderr.log")
        );
        // 4 bundled + 15 standard (incl. host_mcp_cache) = 19
        // + config + data + global_ext + project_elph + project_ext = 19+2+1+1+1 = 24
        assert_eq!(paths.standard_required_dirs().len(), 19);
        assert_eq!(paths.required_dirs().len(), 24);
    }

    #[test]
    fn session_artifact_dir_from_db_joins_projects() {
        let dir = Paths::session_artifact_dir_from_db(std::path::Path::new("/data/metadata.db"), "sess1");
        assert_eq!(dir, PathBuf::from("/data/projects/sess1"));
    }
}
