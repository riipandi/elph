//! Run mode: code (repository `openwiki/`) vs personal (`~/.owly/wiki`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::constants::OWLY_DIR;
use crate::credentials;

/// Owly product mode (maps upstream `openwiki code` vs `openwiki personal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunMode {
    #[default]
    Code,
    Personal,
}

/// Agent output routing (upstream `repository` vs `local-wiki`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Repository,
    LocalWiki,
}

impl RunMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "code" | "repository" => Some(Self::Code),
            "personal" | "local-wiki" | "local" => Some(Self::Personal),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Personal => "personal",
        }
    }

    pub fn output_mode(self) -> OutputMode {
        match self {
            Self::Code => OutputMode::Repository,
            Self::Personal => OutputMode::LocalWiki,
        }
    }
}

/// Resolved filesystem layout for a run.
#[derive(Debug, Clone)]
pub struct WikiContext {
    pub mode: RunMode,
    /// Repository working directory (code mode) or invoking cwd (personal mode anchor).
    pub repo_cwd: PathBuf,
}

impl WikiContext {
    pub fn new(mode: RunMode, repo_cwd: impl AsRef<Path>) -> Self {
        Self {
            mode,
            repo_cwd: repo_cwd.as_ref().to_path_buf(),
        }
    }

    pub fn code(repo_cwd: impl AsRef<Path>) -> Self {
        Self::new(RunMode::Code, repo_cwd)
    }

    pub fn personal(repo_cwd: impl AsRef<Path>) -> Self {
        Self::new(RunMode::Personal, repo_cwd)
    }

    /// Directory passed to elph-agent `LocalExecutionEnv` (tool root).
    pub fn agent_cwd(&self) -> PathBuf {
        match self.mode {
            RunMode::Code => self.repo_cwd.clone(),
            RunMode::Personal => personal_wiki_root(),
        }
    }

    /// Wiki content root (`openwiki/` or `~/.owly/wiki/`).
    pub fn wiki_root(&self) -> PathBuf {
        match self.mode {
            RunMode::Code => self.repo_cwd.join(OWLY_DIR),
            RunMode::Personal => personal_wiki_root(),
        }
    }

    /// Path used for session thread identity and git helpers in code mode.
    pub fn session_anchor(&self) -> &Path {
        &self.repo_cwd
    }

    pub fn ensure_layout(&self) -> Result<()> {
        match self.mode {
            RunMode::Code => Ok(()),
            RunMode::Personal => ensure_personal_home(),
        }
    }
}

/// Personal wiki output directory (`~/.owly/wiki`).
pub fn personal_wiki_root() -> PathBuf {
    credentials::env_dir().join("wiki")
}

/// Owly home (`~/.owly`).
pub fn owly_home_dir() -> PathBuf {
    credentials::env_dir()
}

/// Create `~/.owly`, `~/.owly/wiki`, and secure permissions.
pub fn ensure_personal_home() -> Result<()> {
    let home = owly_home_dir();
    std::fs::create_dir_all(&home).with_context(|| format!("create {}", home.display()))?;
    std::fs::create_dir_all(personal_wiki_root())
        .with_context(|| format!("create {}", personal_wiki_root().display()))?;
    credentials::secure_env_dir()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_mode_wiki_root_is_openwiki() {
        let ctx = WikiContext::code("/tmp/repo");
        assert_eq!(ctx.wiki_root(), PathBuf::from("/tmp/repo/openwiki"));
        assert_eq!(ctx.agent_cwd(), PathBuf::from("/tmp/repo"));
    }

    #[test]
    fn personal_mode_uses_home_wiki() {
        let ctx = WikiContext::personal("/tmp/any");
        assert!(ctx.wiki_root().ends_with("wiki"));
        assert_eq!(ctx.agent_cwd(), ctx.wiki_root());
    }
}
