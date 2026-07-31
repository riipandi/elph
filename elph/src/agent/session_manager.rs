//! Platform session manager backed by Turso (`APP_DATA/metadata.db`).

use crate::utils::path::AppPaths;
use anyhow::{Context, Result};
use elph_agent::{
    Session, TursoSessionListOptions, TursoSessionMetadata, TursoSessionRepo, TursoSessionRepoCreateOptions,
    TursoSessionStorage, repair_unanswered_tool_calls,
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
            let sessions = self.list().await?;
            if let Some(meta) = sessions.into_iter().find(|s| s.id == id) {
                return self.open(&meta).await;
            }
        }
        let mut session = self
            .repo
            .create(TursoSessionRepoCreateOptions {
                cwd: self.cwd.clone(),
                id: resume_id.map(str::to_string),
                parent_session_id: None,
                system_prompt: None,
                ..Default::default()
            })
            .await
            .context("create session")?;
        let id = session.metadata().await.id;
        self.ensure_artifact_dirs(&id)?;
        // Fresh create has no open tools; still safe if resume id was unknown and became create.
        if let Err(err) = repair_unanswered_tool_calls(&mut session).await {
            log::warn!("session recovery: {err}");
        }
        Ok(session)
    }

    pub async fn list(&self) -> Result<Vec<TursoSessionMetadata>> {
        self.repo
            .list(TursoSessionListOptions {
                cwd: Some(self.cwd.clone()),
            })
            .await
            .context("list sessions")
    }

    pub async fn open(&self, metadata: &TursoSessionMetadata) -> Result<Session<TursoSessionStorage>> {
        let mut session = self.repo.open_metadata(metadata).await.context("open session")?;
        self.ensure_artifact_dirs(&metadata.id)?;
        match repair_unanswered_tool_calls(&mut session).await {
            Ok(report) if report.repaired_tool_results > 0 => {
                log::info!(
                    "session {}: repaired {} interrupted tool result(s)",
                    metadata.id,
                    report.repaired_tool_results
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
