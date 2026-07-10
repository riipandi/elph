//! Multi-file session directory persistence.

use anyhow::{Context, Result};
use elph_agent::{
    LocalExecutionEnv, Session, SessionDirListOptions, SessionDirMetadata, SessionDirRepo, SessionDirRepoCreateOptions,
    SessionDirStorage,
};
use elph_core::utils::path::AppPaths;
use elph_core::utils::project_key;
use std::path::Path;
use std::sync::Arc;

use crate::platform::Paths;

pub struct SessionManager {
    repo: SessionDirRepo,
    cwd: String,
    project_key: String,
}

impl SessionManager {
    pub fn new(paths: &Paths, env: Arc<LocalExecutionEnv>, cwd: &Path) -> Result<Self> {
        let project_key = project_key::from_path(cwd)?;
        let sessions_root = paths.sessions_dir().to_string_lossy().to_string();
        Ok(Self {
            repo: SessionDirRepo::new(env, sessions_root, project_key.clone()),
            cwd: cwd.display().to_string(),
            project_key,
        })
    }

    pub async fn create(&self, resume_id: Option<&str>) -> Result<Session<SessionDirStorage>> {
        if let Some(id) = resume_id {
            let sessions = self.list().await?;
            if let Some(meta) = sessions.into_iter().find(|s| s.id == id) {
                return self.open(&meta).await;
            }
        }
        self.repo
            .create(SessionDirRepoCreateOptions {
                cwd: self.cwd.clone(),
                project_key: self.project_key.clone(),
                id: resume_id.map(str::to_string),
                parent_session_id: None,
                system_prompt: None,
            })
            .await
            .context("create session")
    }

    pub async fn list(&self) -> Result<Vec<SessionDirMetadata>> {
        self.repo
            .list(SessionDirListOptions {
                cwd: Some(self.cwd.clone()),
                project_key: Some(self.project_key.clone()),
            })
            .await
            .context("list sessions")
    }

    pub async fn open(&self, metadata: &SessionDirMetadata) -> Result<Session<SessionDirStorage>> {
        self.repo.open(metadata).await.context("open session")
    }

    pub async fn delete(&self, metadata: &SessionDirMetadata) -> Result<()> {
        self.repo.delete(metadata).await.context("delete session")
    }

    pub fn project_key(&self) -> &str {
        &self.project_key
    }
}
