//! JSONL session persistence wrapper.

use anyhow::{Context, Result};
use elph_agent::{
    JsonlSessionListOptions, JsonlSessionMetadata, JsonlSessionRepo, JsonlSessionRepoCreateOptions,
    JsonlSessionStorage, LocalExecutionEnv, Session,
};
use elph_core::utils::path::AppPaths;
use std::path::Path;
use std::sync::Arc;

use crate::runtime::Paths;

pub struct SessionManager {
    repo: JsonlSessionRepo,
    cwd: String,
}

impl SessionManager {
    pub fn new(paths: &Paths, env: Arc<LocalExecutionEnv>, cwd: &Path) -> Self {
        let sessions_root = paths.sessions_dir().to_string_lossy().to_string();
        Self {
            repo: JsonlSessionRepo::new(env, sessions_root),
            cwd: cwd.display().to_string(),
        }
    }

    pub async fn create(&self, resume_id: Option<&str>) -> Result<Session<JsonlSessionStorage>> {
        if let Some(id) = resume_id {
            let sessions = self.list().await?;
            if let Some(meta) = sessions.into_iter().find(|s| s.id == id) {
                return self.open(&meta).await;
            }
        }
        self.repo
            .create(JsonlSessionRepoCreateOptions {
                cwd: self.cwd.clone(),
                id: resume_id.map(str::to_string),
                parent_session_path: None,
            })
            .await
            .context("create session")
    }

    pub async fn list(&self) -> Result<Vec<JsonlSessionMetadata>> {
        self.repo
            .list(JsonlSessionListOptions {
                cwd: Some(self.cwd.clone()),
            })
            .await
            .context("list sessions")
    }

    pub async fn open(&self, metadata: &JsonlSessionMetadata) -> Result<Session<JsonlSessionStorage>> {
        self.repo.open(metadata).await.context("open session")
    }

    pub async fn delete(&self, metadata: &JsonlSessionMetadata) -> Result<()> {
        self.repo.delete(metadata).await.context("delete session")
    }
}
