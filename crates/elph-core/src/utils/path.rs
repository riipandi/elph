//! Generic home, data, and project directory resolution.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Environment and naming knobs for an application layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathResolver {
    /// Override env var for the config/home directory (e.g. `ELPH_HOME`).
    pub home_env: &'static str,
    /// Override env var for the data directory (e.g. `ELPH_DATA_DIR`).
    pub data_env: &'static str,
    /// Override env var for the project directory (e.g. `ELPH_PROJECT_DIR`).
    pub project_env: &'static str,
    /// Config directory name under `$HOME` (e.g. `.elph`).
    pub config_dir_name: &'static str,
    /// Data directory name under XDG data home (e.g. `elph`).
    pub data_dir_name: &'static str,
}

/// Resolved config, data, and project directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub project_dir: PathBuf,
}

impl PathResolver {
    pub fn resolve(&self) -> Result<ResolvedPaths> {
        Ok(ResolvedPaths::from_dirs(
            self.config_dir()?,
            self.data_dir()?,
            self.project_dir()?,
        ))
    }

    fn config_dir(&self) -> Result<PathBuf> {
        if let Some(path) = env_path(self.home_env) {
            return Ok(path);
        }

        Ok(user_home()?.join(self.config_dir_name))
    }

    fn data_dir(&self) -> Result<PathBuf> {
        if let Some(path) = env_path(self.data_env) {
            return Ok(path);
        }

        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            let trimmed = xdg.trim();
            if !trimmed.is_empty() {
                return Ok(Path::new(trimmed).join(self.data_dir_name));
            }
        }

        Ok(user_home()?.join(".local").join("share").join(self.data_dir_name))
    }

    fn project_dir(&self) -> Result<PathBuf> {
        if let Some(path) = env_path(self.project_env) {
            return Ok(path);
        }

        std::env::current_dir().map_err(Into::into)
    }
}

impl ResolvedPaths {
    pub fn from_dirs(config_dir: PathBuf, data_dir: PathBuf, project_dir: PathBuf) -> Self {
        Self {
            config_dir,
            data_dir,
            project_dir,
        }
    }
}

/// Common config/data path helpers shared by Elph applications.
pub trait AppPaths {
    fn config_dir(&self) -> &PathBuf;
    fn data_dir(&self) -> &PathBuf;

    fn settings_path(&self) -> PathBuf {
        self.config_dir().join("settings.json")
    }

    fn trust_path(&self) -> PathBuf {
        self.config_dir().join("trust.json")
    }

    fn bundled_dir(&self) -> PathBuf {
        self.config_dir().join("bundled")
    }

    fn bundled_manifest_path(&self) -> PathBuf {
        self.bundled_dir().join("manifest.json")
    }

    fn prompts_dir(&self) -> PathBuf {
        self.config_dir().join("prompts")
    }

    fn providers_dir(&self) -> PathBuf {
        self.config_dir().join("providers")
    }

    fn sessions_dir(&self) -> PathBuf {
        self.config_dir().join("sessions")
    }

    fn skills_dir(&self) -> PathBuf {
        self.config_dir().join("skills")
    }

    fn worktrees_dir(&self) -> PathBuf {
        self.config_dir().join("worktrees")
    }

    fn attachments_dir(&self) -> PathBuf {
        self.data_dir().join("attachments")
    }

    fn downloads_dir(&self) -> PathBuf {
        self.data_dir().join("downloads")
    }

    fn logs_dir(&self) -> PathBuf {
        self.data_dir().join("logs")
    }

    fn vendor_dir(&self) -> PathBuf {
        self.data_dir().join("vendor")
    }

    fn metadata_db_path(&self) -> PathBuf {
        self.data_dir().join("metadata.db")
    }

    fn version_path(&self) -> PathBuf {
        self.data_dir().join("version.json")
    }

    fn bundled_content_dirs(&self) -> [PathBuf; 4] {
        let bundled = self.bundled_dir();
        [
            bundled.join("agents"),
            bundled.join("personas"),
            bundled.join("skills"),
            bundled.join("user-guide"),
        ]
    }

    fn standard_required_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = self.bundled_content_dirs().into_iter().collect::<Vec<_>>();
        dirs.extend([
            self.prompts_dir(),
            self.providers_dir(),
            self.sessions_dir(),
            self.skills_dir(),
            self.worktrees_dir(),
            self.attachments_dir(),
            self.downloads_dir(),
            self.logs_dir(),
            self.vendor_dir(),
        ]);
        dirs
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var(name).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn user_home() -> Result<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).context("HOME is not set")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_RESOLVER: PathResolver = PathResolver {
        home_env: "TEST_AGENT_HOME",
        data_env: "TEST_AGENT_DATA",
        project_env: "TEST_AGENT_PROJECT",
        config_dir_name: ".test-agent",
        data_dir_name: "test-agent",
    };

    struct TestPaths {
        config_dir: PathBuf,
        data_dir: PathBuf,
    }

    impl AppPaths for TestPaths {
        fn config_dir(&self) -> &PathBuf {
            &self.config_dir
        }

        fn data_dir(&self) -> &PathBuf {
            &self.data_dir
        }
    }

    #[test]
    fn resolves_from_explicit_dirs() {
        let paths = ResolvedPaths::from_dirs(PathBuf::from("/cfg"), PathBuf::from("/data"), PathBuf::from("/repo"));

        assert_eq!(paths.config_dir, PathBuf::from("/cfg"));
        assert_eq!(paths.data_dir, PathBuf::from("/data"));
        assert_eq!(paths.project_dir, PathBuf::from("/repo"));
    }

    #[test]
    fn resolver_exposes_static_names() {
        assert_eq!(TEST_RESOLVER.config_dir_name, ".test-agent");
        assert_eq!(TEST_RESOLVER.data_dir_name, "test-agent");
    }

    #[test]
    fn app_paths_builds_expected_file_paths() {
        let paths = TestPaths {
            config_dir: PathBuf::from("/cfg"),
            data_dir: PathBuf::from("/data"),
        };

        assert_eq!(paths.metadata_db_path(), PathBuf::from("/data/metadata.db"));
        assert_eq!(
            paths.bundled_manifest_path(),
            PathBuf::from("/cfg/bundled/manifest.json")
        );
        assert_eq!(paths.standard_required_dirs().len(), 13);
    }
}
