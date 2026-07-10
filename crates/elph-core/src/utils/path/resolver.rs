//! Generic home, data, and project directory resolution.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Environment and naming knobs for an application's home directories.
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
