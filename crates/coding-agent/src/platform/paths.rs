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

    /// Project-local unified store (Turso DB).
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
        dirs
    }

    /// `CONFIG_DIR/extensions/`
    pub fn global_extensions_dir(&self) -> PathBuf {
        elph_agent::plugins::global_extensions_dir(self.config_dir())
    }

    /// `<project>/.elph/extensions/`
    pub fn project_extensions_dir(&self) -> PathBuf {
        elph_agent::plugins::project_extensions_dir(&self.project_elph_dir())
    }

    /// Project MCP override: `<project>/.elph/mcp.json` (merged over home `mcp.json`).
    pub fn project_mcp_config_path(&self) -> PathBuf {
        self.project_elph_dir().join("mcp.json")
    }

    /// Project settings override: `<project>/.elph/settings.json` (merged over home settings).
    pub fn project_settings_path(&self) -> PathBuf {
        self.project_elph_dir().join("settings.json")
    }

    /// One-time move of legacy `APP_DATA/projects/*` → `APP_DATA/sessions/*`.
    ///
    /// Safe to call every boot: only renames children that do not already exist under `sessions/`.
    pub fn migrate_projects_to_sessions(&self) -> std::io::Result<()> {
        let projects = self.data_dir().join("projects");
        if !projects.is_dir() {
            return Ok(());
        }
        let sessions = self.sessions_dir();
        std::fs::create_dir_all(&sessions)?;
        for entry in std::fs::read_dir(&projects)? {
            let entry = entry?;
            let dest = sessions.join(entry.file_name());
            if dest.exists() {
                continue;
            }
            // Prefer rename (same volume); fall back to recursive copy+remove if needed.
            if std::fs::rename(entry.path(), &dest).is_err() {
                copy_dir_recursive(&entry.path(), &dest)?;
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
        // Drop empty legacy root when possible.
        let _ = std::fs::remove_dir(&projects);
        Ok(())
    }

    /// One-time move of legacy project-local tool outputs
    /// (`<project>/.elph/sessions/<SESSION_ID>/tool_outputs.jsonl`) into the
    /// APP_DATA session dir (`~/.local/share/elph/sessions/<SESSION_ID>/`).
    ///
    /// Safe to call every boot: only moves files whose destination does not
    /// already exist, and removes the legacy tree once empty.
    pub fn migrate_legacy_session_tool_outputs(&self) -> std::io::Result<()> {
        let legacy_root = self.project_elph_dir().join("sessions");
        if !legacy_root.is_dir() {
            return Ok(());
        }
        let sessions = self.sessions_dir();
        for entry in std::fs::read_dir(&legacy_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let src = entry.path().join("tool_outputs.jsonl");
            if !src.is_file() {
                continue;
            }
            let dest = sessions.join(entry.file_name()).join("tool_outputs.jsonl");
            if dest.exists() {
                continue;
            }
            std::fs::create_dir_all(dest.parent().expect("dest parent"))?;
            std::fs::rename(&src, &dest)?;
            // Drop the legacy session dir when empty.
            let _ = std::fs::remove_dir(entry.path());
        }
        // Drop empty legacy root when possible.
        let _ = std::fs::remove_dir(&legacy_root);
        Ok(())
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
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

        assert_eq!(paths.memory_db_path(), project.join(".elph/store.db"));
        assert_eq!(paths.project_gitignore_path(), project.join(".elph/.gitignore"));
        assert_eq!(paths.project_settings_path(), project.join(".elph/settings.json"));
        assert_eq!(paths.project_mcp_config_path(), project.join(".elph/mcp.json"));
        assert_eq!(paths.bundled_manifest_path(), config.join("bundled/manifest.json"));
        assert_eq!(paths.agents_dir(), config.join("agents"));
        assert_eq!(paths.hooks_dir(), config.join("hooks"));
        assert_eq!(paths.host_mcp_cache_dir(), data.join("mcp_cache"));
        assert_eq!(paths.worktrees_dir(), data.join("worktrees"));
        assert_eq!(paths.sessions_dir(), data.join("sessions"));
        assert_eq!(paths.models_dir(), data.join("models"));
        assert_eq!(paths.session_artifact_dir("abc123"), data.join("sessions").join("abc123"));
        assert_eq!(paths.session_mcp_cache_dir("abc123"), data.join("sessions/abc123/mcp_cache"));
        assert_eq!(
            paths.mcp_tool_stderr_log_path("my server", "tool/name"),
            data.join("logs/mcp/my_server/tool_name.stderr.log")
        );
        // 3 bundled + 14 standard (sessions, no projects) = 17
        // + config + data + global_ext + project_elph = 17+2+1+1 = 21
        assert_eq!(paths.standard_required_dirs().len(), 17);
        assert_eq!(paths.required_dirs().len(), 21);
    }

    #[test]
    fn migrate_projects_to_sessions_moves_children() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let data = tmp.path().join("data");
        let projects = data.join("projects");
        let sid = projects.join("abc");
        std::fs::create_dir_all(sid.join("mcp_cache")).expect("mkdir");
        std::fs::write(sid.join("tool_outputs.jsonl"), b"{}").expect("write");
        let paths = Paths::from_dirs(tmp.path().join("cfg"), data.clone(), tmp.path().join("repo"));
        paths.migrate_projects_to_sessions().expect("migrate");
        assert!(data.join("sessions/abc/mcp_cache").is_dir());
        assert!(data.join("sessions/abc/tool_outputs.jsonl").is_file());
        assert!(
            !projects.is_dir()
                || std::fs::read_dir(&projects)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(true)
        );
    }

    #[test]
    fn migrate_legacy_session_tool_outputs_moves_to_app_data() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let data = tmp.path().join("data");
        let project = tmp.path().join("repo");
        let legacy = project.join(".elph/sessions/abc");
        std::fs::create_dir_all(&legacy).expect("mkdir");
        std::fs::write(legacy.join("tool_outputs.jsonl"), b"legacy").expect("write");
        let paths = Paths::from_dirs(tmp.path().join("cfg"), data.clone(), project.clone());
        paths.migrate_legacy_session_tool_outputs().expect("migrate");
        assert!(!legacy.join("tool_outputs.jsonl").exists());
        let dest = data.join("sessions/abc/tool_outputs.jsonl");
        assert!(dest.is_file());
        assert_eq!(std::fs::read_to_string(dest).expect("read"), "legacy");
        // Legacy root is dropped once empty.
        assert!(!project.join(".elph/sessions").exists());
    }

    #[test]
    fn migrate_legacy_session_tool_outputs_keeps_existing_destination() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let data = tmp.path().join("data");
        let project = tmp.path().join("repo");
        let legacy = project.join(".elph/sessions/abc");
        std::fs::create_dir_all(&legacy).expect("mkdir");
        std::fs::write(legacy.join("tool_outputs.jsonl"), b"legacy").expect("write");
        std::fs::create_dir_all(data.join("sessions/abc")).expect("mkdir");
        std::fs::write(data.join("sessions/abc/tool_outputs.jsonl"), b"current").expect("write");
        let paths = Paths::from_dirs(tmp.path().join("cfg"), data.clone(), project.clone());
        paths.migrate_legacy_session_tool_outputs().expect("migrate");
        assert_eq!(
            std::fs::read_to_string(data.join("sessions/abc/tool_outputs.jsonl")).expect("read"),
            "current"
        );
    }
}
