//! Resolve TUI / run session resume vs continue for the current project.

use std::path::Path;

use crate::agent::SessionManager;
use crate::platform::Paths;

/// How the interactive default (or `run`) should open a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLaunchMode {
    /// Fresh session for the project.
    New,
    /// Resume by explicit session id (`--resume` / `-r`) — must already exist.
    Resume { session_id: String },
    /// Resume most recent session for this project (`--continue` / `-c`).
    Continue,
    /// Open `session_id` if present, otherwise create a session with that id (`--session-id`).
    OpenOrCreate { session_id: String },
}

impl SessionLaunchMode {
    /// Build from global / run CLI flags.
    ///
    /// Errors when flags conflict (`--continue` + `--resume`, `--no-session` with any resume).
    pub fn from_run_flags(
        continue_session: bool,
        resume: Option<String>,
        session_id: Option<String>,
        no_session: bool,
    ) -> Result<Self, String> {
        if no_session
            && (continue_session || resume.is_some() || session_id.as_ref().is_some_and(|s| !s.trim().is_empty()))
        {
            return Err(
                "cannot combine --no-session with --continue, --resume, or --session-id (nothing to resume)".into(),
            );
        }

        let resume = match resume {
            Some(s) => {
                let s = s.trim().to_string();
                if s.is_empty() {
                    return Err("--resume requires a non-empty SESSION_ID".into());
                }
                Some(s)
            }
            None => None,
        };
        let session_id = match session_id {
            Some(s) => {
                let s = s.trim().to_string();
                if s.is_empty() {
                    return Err("--session-id requires a non-empty ID".into());
                }
                Some(s)
            }
            None => None,
        };

        match (continue_session, resume, session_id) {
            (true, Some(_), _) | (true, _, Some(_)) => {
                Err("cannot use --continue together with --resume or --session-id; pick one".into())
            }
            (false, Some(_), Some(_)) => {
                Err("cannot use --resume and --session-id together; pick one (must-exist vs create-or-open)".into())
            }
            (true, None, None) => Ok(Self::Continue),
            (false, Some(id), None) => Ok(Self::Resume { session_id: id }),
            (false, None, Some(id)) => Ok(Self::OpenOrCreate { session_id: id }),
            (false, None, None) => Ok(Self::New),
        }
    }

    /// Build from global CLI flags (TUI). Errors if `--continue` and `--resume` are both set.
    pub fn from_flags(continue_session: bool, resume: Option<String>) -> Result<Self, String> {
        Self::from_run_flags(continue_session, resume, None, false)
    }

    /// Resolve to a `resume_id` and whether missing ids should be **created**.
    ///
    /// Returns `(resume_or_create_id, create_if_missing)`.
    pub async fn resolve(&self, paths: &Paths, project_dir: &Path) -> Result<(Option<String>, bool), String> {
        match self {
            Self::New => Ok((None, false)),
            Self::Resume { session_id } => {
                let manager = SessionManager::new(paths, project_dir).map_err(|e| e.to_string())?;
                match manager.find_metadata(session_id).await {
                    Ok(Some(meta)) => Ok((Some(meta.id), false)),
                    Ok(None) => Err(format!("session not found: {session_id}\n  Hint: elph session list")),
                    Err(e) => Err(format!("lookup session: {e}")),
                }
            }
            Self::OpenOrCreate { session_id } => {
                let manager = SessionManager::new(paths, project_dir).map_err(|e| e.to_string())?;
                match manager.find_metadata(session_id).await {
                    Ok(Some(meta)) => Ok((Some(meta.id), false)),
                    Ok(None) => Ok((Some(session_id.clone()), true)),
                    Err(e) => Err(format!("lookup session: {e}")),
                }
            }
            Self::Continue => {
                let manager = SessionManager::new(paths, project_dir).map_err(|e| e.to_string())?;
                match manager.latest_session_id().await {
                    Ok(Some(id)) => {
                        log::info!("continuing last project session: id={id} cwd={}", project_dir.display());
                        Ok((Some(id), false))
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

    /// Resolve to a `resume_id` for [`crate::agent::SessionManager::create`].
    ///
    /// - `New` → `None` (creates a new session)
    /// - `Resume` → `Some(id)` if found
    /// - `Continue` → `Some(latest)` or error if this project has no sessions
    ///
    /// Prefer [`Self::resolve`] for headless `--session-id` create-or-open.
    pub async fn resolve_resume_id(&self, paths: &Paths, project_dir: &Path) -> Result<Option<String>, String> {
        let (id, create_if_missing) = self.resolve(paths, project_dir).await?;
        if create_if_missing {
            // OpenOrCreate with missing id: pass through for create_with_id.
            return Ok(id);
        }
        Ok(id)
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

    #[test]
    fn no_session_conflicts_with_resume() {
        let err = SessionLaunchMode::from_run_flags(false, Some("x".into()), None, true).unwrap_err();
        assert!(err.contains("--no-session"));
    }

    #[test]
    fn session_id_open_or_create() {
        assert_eq!(
            SessionLaunchMode::from_run_flags(false, None, Some("fixed-id".into()), false).unwrap(),
            SessionLaunchMode::OpenOrCreate {
                session_id: "fixed-id".into()
            }
        );
    }

    #[test]
    fn resume_and_session_id_conflict() {
        assert!(SessionLaunchMode::from_run_flags(false, Some("a".into()), Some("b".into()), false).is_err());
    }
}
