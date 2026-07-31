//! Resolve TUI / run session resume vs continue for the current project.

use std::path::Path;

use crate::agent::SessionManager;
use crate::platform::Paths;

/// How the interactive default (or `run`) should open a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLaunchMode {
    /// Fresh session for the project.
    New,
    /// Resume by explicit session id (`--resume` / `-r`).
    Resume { session_id: String },
    /// Resume most recent session for this project (`--continue` / `-c`).
    Continue,
}

impl SessionLaunchMode {
    /// Build from global CLI flags. Errors if `--continue` and `--resume` are both set.
    pub fn from_flags(continue_session: bool, resume: Option<String>) -> Result<Self, String> {
        match (continue_session, resume) {
            (true, Some(_)) => Err(
                "cannot use --continue and --resume together; pick one (latest project session vs explicit id)".into(),
            ),
            (true, None) => Ok(Self::Continue),
            (false, Some(id)) => {
                let id = id.trim().to_string();
                if id.is_empty() {
                    Err("--resume requires a non-empty SESSION_ID".into())
                } else {
                    Ok(Self::Resume { session_id: id })
                }
            }
            (false, None) => Ok(Self::New),
        }
    }

    /// Resolve to a `resume_id` for [`crate::agent::SessionManager::create`].
    ///
    /// - `New` → `None` (creates a new session)
    /// - `Resume` → `Some(id)` if found
    /// - `Continue` → `Some(latest)` or error if this project has no sessions
    pub async fn resolve_resume_id(&self, paths: &Paths, project_dir: &Path) -> Result<Option<String>, String> {
        match self {
            Self::New => Ok(None),
            Self::Resume { session_id } => {
                let manager = SessionManager::new(paths, project_dir).map_err(|e| e.to_string())?;
                match manager.find_metadata(session_id).await {
                    Ok(Some(meta)) => Ok(Some(meta.id)),
                    Ok(None) => Err(format!("session not found: {session_id}\n  Hint: elph session list")),
                    Err(e) => Err(format!("lookup session: {e}")),
                }
            }
            Self::Continue => {
                let manager = SessionManager::new(paths, project_dir).map_err(|e| e.to_string())?;
                match manager.latest_session_id().await {
                    Ok(Some(id)) => {
                        log::info!("continuing last project session: id={id} cwd={}", project_dir.display());
                        Ok(Some(id))
                    }
                    Ok(None) => Err(format!(
                        "no sessions found for this project ({})\n  Start one with: elph\n  Or list: elph session list",
                        project_dir.display()
                    )),
                    Err(e) => Err(format!("list sessions: {e}")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_flags_continue_and_resume_conflict() {
        let err = SessionLaunchMode::from_flags(true, Some("abc".into())).unwrap_err();
        assert!(err.contains("cannot use"));
    }

    #[test]
    fn from_flags_continue_only() {
        assert_eq!(SessionLaunchMode::from_flags(true, None).unwrap(), SessionLaunchMode::Continue);
    }

    #[test]
    fn from_flags_resume_only() {
        assert_eq!(
            SessionLaunchMode::from_flags(false, Some("sess-1".into())).unwrap(),
            SessionLaunchMode::Resume {
                session_id: "sess-1".into()
            }
        );
    }

    #[test]
    fn from_flags_new() {
        assert_eq!(SessionLaunchMode::from_flags(false, None).unwrap(), SessionLaunchMode::New);
    }

    #[test]
    fn from_flags_resume_empty_rejected() {
        assert!(SessionLaunchMode::from_flags(false, Some("  ".into())).is_err());
    }
}
