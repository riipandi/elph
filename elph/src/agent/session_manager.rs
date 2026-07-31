//! Platform session manager backed by Turso (`APP_DATA/metadata.db`).

use crate::utils::path::AppPaths;
use anyhow::{Context, Result};
use elph_agent::{
    Session, TursoSessionListOptions, TursoSessionMetadata, TursoSessionRepo, TursoSessionRepoCreateOptions,
    TursoSessionStorage, reconcile_session,
};
use std::fs;
use std::path::{Path, PathBuf};

use crate::platform::Paths;

pub struct SessionManager {
    repo: TursoSessionRepo,
    cwd: String,
    /// `APP_DATA` root — used for `projects/<SESSION_ID>/` artifacts.
    data_dir: PathBuf,
}

impl SessionManager {
    pub fn new(paths: &Paths, cwd: &Path) -> Result<Self> {
        Ok(Self {
            repo: TursoSessionRepo::new(paths.metadata_db_path()),
            cwd: cwd.display().to_string(),
            data_dir: paths.data_dir().clone(),
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn artifact_dir_for(&self, session_id: &str) -> PathBuf {
        self.data_dir.join("projects").join(session_id)
    }

    fn ensure_artifact_dirs(&self, session_id: &str) -> Result<()> {
        let base = self.artifact_dir_for(session_id);
        for sub in ["mcp_cache", "terminals"] {
            fs::create_dir_all(base.join(sub)).with_context(|| format!("create session {sub}"))?;
        }
        Ok(())
    }

    fn remove_artifact_dirs(&self, session_id: &str) {
        let base = self.artifact_dir_for(session_id);
        if base.exists() {
            let _ = fs::remove_dir_all(&base);
        }
    }

    pub async fn create(&self, resume_id: Option<&str>) -> Result<Session<TursoSessionStorage>> {
        if let Some(id) = resume_id {
            if let Some(meta) = self.find_metadata(id).await? {
                return self.open(&meta).await;
            }
            anyhow::bail!(
                "session not found: {id} (use `elph session list` or `elph --continue` for the latest in this project)"
            );
        }
        let mut session = self
            .repo
            .create(TursoSessionRepoCreateOptions {
                cwd: self.cwd.clone(),
                id: None,
                parent_session_id: None,
                system_prompt: None,
                ..Default::default()
            })
            .await
            .context("create session")?;
        let id = session.metadata().await.id;
        self.ensure_artifact_dirs(&id)?;
        // Reconcile is cheap on empty sessions; full restore also runs in AgentHarness::restore.
        if let Err(err) = reconcile_session(&mut session).await {
            log::warn!("session recovery: {err}");
        }
        Ok(session)
    }

    /// Sessions for this manager's project cwd, newest `updated_at` first.
    pub async fn list(&self) -> Result<Vec<TursoSessionMetadata>> {
        self.repo
            .list(TursoSessionListOptions {
                cwd: Some(self.cwd.clone()),
            })
            .await
            .context("list sessions")
    }

    /// Most recently updated session id for this project, if any.
    pub async fn latest_session_id(&self) -> Result<Option<String>> {
        Ok(self.list().await?.into_iter().next().map(|m| m.id))
    }

    /// Find session metadata by id (this project first, then global).
    pub async fn find_metadata(&self, session_id: &str) -> Result<Option<TursoSessionMetadata>> {
        if let Some(meta) = self.list().await?.into_iter().find(|s| s.id == session_id) {
            return Ok(Some(meta));
        }
        let all = self
            .repo
            .list(TursoSessionListOptions { cwd: None })
            .await
            .context("list all sessions")?;
        Ok(all.into_iter().find(|s| s.id == session_id))
    }

    pub async fn open(&self, metadata: &TursoSessionMetadata) -> Result<Session<TursoSessionStorage>> {
        let mut session = self.repo.open_metadata(metadata).await.context("open session")?;
        self.ensure_artifact_dirs(&metadata.id)?;
        match reconcile_session(&mut session).await {
            Ok(report) if report.repaired_tool_results > 0 || report.closed_operations > 0 => {
                log::info!(
                    "session {}: repaired {} tool result(s), closed {} open op(s)",
                    metadata.id,
                    report.repaired_tool_results,
                    report.closed_operations
                );
            }
            Err(err) => log::warn!("session recovery: {err}"),
            _ => {}
        }
        Ok(session)
    }

    pub async fn delete(&self, metadata: &TursoSessionMetadata) -> Result<()> {
        self.repo.delete_metadata(metadata).await.context("delete session")?;
        self.remove_artifact_dirs(&metadata.id);
        Ok(())
    }

    pub async fn delete_by_id(&self, session_id: &str) -> Result<()> {
        self.repo.delete(session_id).await.context("delete session")?;
        self.remove_artifact_dirs(session_id);
        Ok(())
    }
}
