//! Session repositories for creating, opening, listing, forking, and deleting sessions.

use std::collections::HashMap;
use std::sync::Arc;

use crate::env::LocalExecutionEnv;
use crate::harness::types::{FileSystem, get_or_throw};
use crate::session::backends::InMemorySessionStorage;
use crate::session::backends::session_dir::{
    SUMMARY_FILE, SessionDirCreateOptions, SessionDirStorage, load_session_metadata,
};
use crate::session::repo_utils::{ForkEntriesOptions, create_session_id, get_entries_to_fork, to_session};
use crate::session::tree::Session;
use crate::session::types::{SessionDirMetadata, SessionError, SessionErrorCode, SessionMetadata, SessionStorage};

#[derive(Debug, Clone, Default)]
pub struct InMemorySessionCreateOptions {
    pub id: Option<String>,
}

pub struct InMemorySessionRepo {
    sessions: HashMap<String, Session<InMemorySessionStorage>>,
}

impl Default for InMemorySessionRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemorySessionRepo {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub async fn create(
        &mut self,
        options: InMemorySessionCreateOptions,
    ) -> Result<Session<InMemorySessionStorage>, SessionError> {
        let metadata = SessionMetadata {
            id: options.id.unwrap_or_else(create_session_id),
            created_at: crate::session::repo_utils::create_timestamp(),
        };
        let storage = InMemorySessionStorage::new(Some(crate::session::backends::InMemorySessionOptions {
            metadata: Some(metadata.clone()),
            ..Default::default()
        }))?;
        let session = to_session(storage);
        self.sessions.insert(metadata.id.clone(), session.clone());
        Ok(session)
    }

    pub async fn open(&self, metadata: &SessionMetadata) -> Result<Session<InMemorySessionStorage>, SessionError> {
        self.sessions
            .get(&metadata.id)
            .cloned()
            .ok_or_else(|| SessionError::new(SessionErrorCode::NotFound, format!("Session not found: {}", metadata.id)))
    }

    pub async fn list(&self) -> Vec<SessionMetadata> {
        let mut sessions = Vec::new();
        for session in self.sessions.values() {
            sessions.push(session.metadata().await);
        }
        sessions
    }

    pub async fn delete(&mut self, metadata: &SessionMetadata) {
        self.sessions.remove(&metadata.id);
    }

    pub async fn fork(
        &mut self,
        source_metadata: &SessionMetadata,
        options: ForkEntriesOptions,
    ) -> Result<Session<InMemorySessionStorage>, SessionError> {
        let source = self.open(source_metadata).await?;
        let forked_entries = get_entries_to_fork(source.storage(), &options).await?;
        let metadata = SessionMetadata {
            id: options.id.unwrap_or_else(create_session_id),
            created_at: crate::session::repo_utils::create_timestamp(),
        };
        let storage = InMemorySessionStorage::new(Some(crate::session::backends::InMemorySessionOptions {
            metadata: Some(metadata.clone()),
            entries: Some(forked_entries),
            ..Default::default()
        }))?;
        let session = to_session(storage);
        self.sessions.insert(metadata.id.clone(), session.clone());
        Ok(session)
    }
}

#[derive(Debug, Clone)]
pub struct SessionDirRepoCreateOptions {
    pub cwd: String,
    pub project_key: String,
    pub id: Option<String>,
    pub parent_session_id: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionDirListOptions {
    pub cwd: Option<String>,
    pub project_key: Option<String>,
}

pub struct SessionDirRepo {
    fs: Arc<LocalExecutionEnv>,
    sessions_root: String,
    project_key: String,
    resolved_root: tokio::sync::Mutex<Option<String>>,
}

impl SessionDirRepo {
    pub fn new(fs: Arc<LocalExecutionEnv>, sessions_root: impl Into<String>, project_key: impl Into<String>) -> Self {
        Self {
            fs,
            sessions_root: sessions_root.into(),
            project_key: project_key.into(),
            resolved_root: tokio::sync::Mutex::new(None),
        }
    }

    async fn sessions_root(&self) -> Result<String, SessionError> {
        let mut guard = self.resolved_root.lock().await;
        if guard.is_none() {
            *guard = Some(get_or_throw(self.fs.absolute_path(&self.sessions_root, None).await));
        }
        Ok(guard.clone().expect("resolved root"))
    }

    async fn project_sessions_dir(&self, project_key: &str) -> Result<String, SessionError> {
        let root = self.sessions_root().await?;
        Ok(get_or_throw(self.fs.join_path(&[root.as_str(), project_key], None).await))
    }

    async fn session_dir(&self, project_key: &str, session_id: &str) -> Result<String, SessionError> {
        let project_dir = self.project_sessions_dir(project_key).await?;
        Ok(get_or_throw(self.fs.join_path(&[project_dir.as_str(), session_id], None).await))
    }

    pub async fn create(
        &self,
        options: SessionDirRepoCreateOptions,
    ) -> Result<Session<SessionDirStorage>, SessionError> {
        let id = options.id.unwrap_or_else(create_session_id);
        let session_dir = self.session_dir(&options.project_key, &id).await?;
        let storage = SessionDirStorage::create(
            &session_dir,
            SessionDirCreateOptions {
                cwd: options.cwd,
                session_id: id,
                parent_session_id: options.parent_session_id,
                system_prompt: options.system_prompt,
            },
        )
        .await?;
        Ok(to_session(storage))
    }

    pub async fn open(&self, metadata: &SessionDirMetadata) -> Result<Session<SessionDirStorage>, SessionError> {
        if !get_or_throw(self.fs.exists(&metadata.dir, None).await) {
            return Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Session not found: {}", metadata.dir),
            ));
        }
        Ok(to_session(SessionDirStorage::open(&metadata.dir).await?))
    }

    pub async fn list(&self, options: SessionDirListOptions) -> Result<Vec<SessionDirMetadata>, SessionError> {
        let project_dirs = if let Some(key) = options.project_key {
            vec![self.project_sessions_dir(&key).await?]
        } else {
            self.list_project_session_dirs().await?
        };
        let cwd_filter = options.cwd.as_deref();
        let mut sessions = Vec::new();
        for project_dir in project_dirs {
            if !get_or_throw(self.fs.exists(&project_dir, None).await) {
                continue;
            }
            let entries = get_or_throw(self.fs.list_dir(&project_dir, None).await);
            for entry in entries {
                if entry.kind != crate::harness::types::FileKind::Directory {
                    continue;
                }
                let summary_path = get_or_throw(self.fs.join_path(&[entry.path.as_str(), SUMMARY_FILE], None).await);
                if !get_or_throw(self.fs.exists(&summary_path, None).await) {
                    continue;
                }
                match load_session_metadata(&entry.path).await {
                    Ok(metadata) => {
                        if cwd_filter.is_none_or(|cwd| metadata.cwd == cwd) {
                            sessions.push(metadata);
                        }
                    }
                    Err(error) if error.code == SessionErrorCode::InvalidSession => {}
                    Err(error) => return Err(error),
                }
            }
        }
        sessions.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(sessions)
    }

    pub async fn delete(&self, metadata: &SessionDirMetadata) -> Result<(), SessionError> {
        get_or_throw(
            self.fs
                .remove(
                    &metadata.dir,
                    Some(crate::harness::types::RemoveOptions {
                        recursive: true,
                        force: true,
                        abort_token: None,
                    }),
                )
                .await,
        );
        Ok(())
    }

    pub async fn fork(
        &self,
        source_metadata: &SessionDirMetadata,
        options: SessionDirRepoCreateOptions,
        fork_options: ForkEntriesOptions,
    ) -> Result<Session<SessionDirStorage>, SessionError> {
        let source = self.open(source_metadata).await?;
        let forked_entries = get_entries_to_fork(source.storage(), &fork_options).await?;
        let id = options.id.unwrap_or_else(create_session_id);
        let session_dir = self.session_dir(&options.project_key, &id).await?;
        let mut storage = SessionDirStorage::create(
            &session_dir,
            SessionDirCreateOptions {
                cwd: options.cwd,
                session_id: id,
                parent_session_id: options.parent_session_id.or_else(|| Some(source_metadata.id.clone())),
                system_prompt: options.system_prompt,
            },
        )
        .await?;
        for entry in forked_entries {
            SessionStorage::append_entry(&mut storage, entry).await?;
        }
        Ok(to_session(storage))
    }

    async fn list_project_session_dirs(&self) -> Result<Vec<String>, SessionError> {
        let root = self.sessions_root().await?;
        if !get_or_throw(self.fs.exists(&root, None).await) {
            return Ok(Vec::new());
        }
        let entries = get_or_throw(self.fs.list_dir(&root, None).await);
        Ok(entries
            .into_iter()
            .filter(|entry| entry.kind == crate::harness::types::FileKind::Directory)
            .map(|entry| entry.path)
            .collect())
    }

    pub fn project_key(&self) -> &str {
        &self.project_key
    }
}
