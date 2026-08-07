//! Multi-session repository over a shared Turso/SQLite database.
//!
//! Analogue of Pi `createSqliteSessionStore` + repository: one DB, many sessions,
//! list/filter by `cwd`, create/open/delete.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::session::backends::turso::{TursoSessionCreateOptions, TursoSessionStorage};
use crate::session::repo_utils::{ForkEntriesOptions, get_entries_to_fork, to_session};
use crate::session::tree::Session;
use crate::session::types::{SessionError, SessionErrorCode, SessionStorage, TursoSessionMetadata};
use turso::Database;

#[derive(Debug, Clone, Default)]
pub struct TursoSessionRepoCreateOptions {
    pub cwd: String,
    pub id: Option<String>,
    pub parent_session_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_mode: Option<String>,
    pub name: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TursoSessionListOptions {
    pub cwd: Option<String>,
}

pub struct TursoSessionRepo {
    db_path: PathBuf,
    /// Shared database handle injected by the host. When present, the repo
    /// connects from this handle instead of opening `db_path` — the host owns
    /// the open/apply-migrations lifetime.
    database: Option<Arc<Database>>,
}

impl TursoSessionRepo {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            database: None,
        }
    }

    /// Attach a shared, already-open database handle. When set, the repo
    /// connects from this handle on each operation instead of opening
    /// [`db_path`] — the host is responsible for opening the database and
    /// applying the session-tree migrations.
    pub fn with_database(mut self, database: Arc<Database>) -> Self {
        self.database = Some(database);
        self
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub async fn create(
        &self,
        options: TursoSessionRepoCreateOptions,
    ) -> Result<Session<TursoSessionStorage>, SessionError> {
        let storage = TursoSessionStorage::create_with_options_with_db(
            &self.db_path,
            TursoSessionCreateOptions {
                session_id: options.id,
                cwd: Some(options.cwd),
                parent_session_id: options.parent_session_id,
                provider_id: options.provider_id,
                model_id: options.model_id,
                agent_mode: options.agent_mode,
                name: options.name,
                system_prompt: options.system_prompt,
                metadata_json: None,
            },
            self.database.clone(),
        )
        .await?;
        Ok(to_session(storage))
    }

    pub async fn open(&self, session_id: &str) -> Result<Session<TursoSessionStorage>, SessionError> {
        Ok(to_session(
            TursoSessionStorage::open(&self.db_path, session_id, self.database.clone()).await?,
        ))
    }

    pub async fn open_metadata(
        &self,
        metadata: &TursoSessionMetadata,
    ) -> Result<Session<TursoSessionStorage>, SessionError> {
        self.open(&metadata.id).await
    }

    pub async fn list(&self, options: TursoSessionListOptions) -> Result<Vec<TursoSessionMetadata>, SessionError> {
        list_sessions(&self.db_path, options.cwd.as_deref(), self.database.as_ref()).await
    }

    /// True when the session tree has at least one stored entry.
    pub async fn has_entries(&self, session_id: &str) -> Result<bool, SessionError> {
        session_has_entries(&self.db_path, session_id, self.database.as_ref()).await
    }

    pub async fn delete(&self, session_id: &str) -> Result<(), SessionError> {
        delete_session(&self.db_path, session_id, self.database.as_ref()).await
    }

    pub async fn delete_metadata(&self, metadata: &TursoSessionMetadata) -> Result<(), SessionError> {
        self.delete(&metadata.id).await
    }

    pub async fn fork(
        &self,
        source_id: &str,
        options: TursoSessionRepoCreateOptions,
        fork_options: ForkEntriesOptions,
    ) -> Result<Session<TursoSessionStorage>, SessionError> {
        let source = self.open(source_id).await?;
        let forked_entries = get_entries_to_fork(source.storage(), &fork_options).await?;
        let parent = options
            .parent_session_id
            .clone()
            .or_else(|| Some(source_id.to_string()));
        let mut session = self
            .create(TursoSessionRepoCreateOptions {
                parent_session_id: parent,
                ..options
            })
            .await?;
        for entry in forked_entries {
            SessionStorage::append_entry(session.storage_mut(), entry).await?;
        }
        Ok(session)
    }
}

async fn open_migrated(db_path: &Path, database: Option<&Arc<Database>>) -> Result<turso::Connection, SessionError> {
    match database {
        Some(db) => db.connect().map_err(map_err),
        None => {
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent).map_err(map_err)?;
            }
            let db = crate::datastore::open_local(db_path).await.map_err(map_err)?;
            let conn = crate::datastore::connect(&db).await.map_err(map_err)?;
            crate::datastore::migrations::run(&conn, &crate::session::migrations::SESSION_TREE_MIGRATIONS)
                .await
                .map_err(|e| SessionError::new(SessionErrorCode::Storage, e.to_string()))?;
            Ok(conn)
        }
    }
}

async fn list_sessions(
    db_path: &Path,
    cwd: Option<&str>,
    database: Option<&Arc<Database>>,
) -> Result<Vec<TursoSessionMetadata>, SessionError> {
    let conn = open_migrated(db_path, database).await?;
    let sql = if cwd.is_some() {
        "SELECT id, created_at, updated_at, cwd, parent_session_id,
                provider_id, model_id, agent_mode, name
         FROM sessions
         WHERE cwd = ?
         ORDER BY updated_at DESC, created_at DESC"
    } else {
        "SELECT id, created_at, updated_at, cwd, parent_session_id,
                provider_id, model_id, agent_mode, name
         FROM sessions
         ORDER BY updated_at DESC, created_at DESC"
    };

    let mut rows = if let Some(cwd) = cwd {
        conn.query(sql, turso::params![cwd]).await.map_err(map_err)?
    } else {
        conn.query(sql, ()).await.map_err(map_err)?
    };

    let db_path_str = db_path.to_string_lossy().to_string();
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(map_err)? {
        out.push(row_to_metadata(&row, &db_path_str)?);
    }
    Ok(out)
}

async fn session_has_entries(
    db_path: &Path,
    session_id: &str,
    database: Option<&Arc<Database>>,
) -> Result<bool, SessionError> {
    let conn = open_migrated(db_path, database).await?;
    let mut rows = conn
        .query(
            "SELECT 1 FROM session_entries WHERE session_id = ? LIMIT 1",
            turso::params![session_id],
        )
        .await
        .map_err(map_err)?;
    Ok(rows.next().await.map_err(map_err)?.is_some())
}

async fn delete_session(
    db_path: &Path,
    session_id: &str,
    database: Option<&Arc<Database>>,
) -> Result<(), SessionError> {
    let conn = open_migrated(db_path, database).await?;
    // Manual cascade: goals FK may not be enforced; wipe tree first then session.
    conn.execute("DELETE FROM session_entries WHERE session_id = ?", turso::params![session_id])
        .await
        .map_err(map_err)?;
    conn.execute("DELETE FROM session_sequences WHERE session_id = ?", turso::params![session_id])
        .await
        .map_err(map_err)?;
    // Goals: best-effort cascade so session delete does not leave orphan goals.
    // The table may be absent in library-only DBs (expected there); any other
    // error (e.g. a lock error) is logged rather than silently swallowed.
    if let Err(error) = conn
        .execute("DELETE FROM goals WHERE session_id = ?", turso::params![session_id])
        .await
    {
        log::warn!("failed to cascade-delete goals for {session_id}: {error}");
    }
    if let Err(error) = conn
        .execute(
            "DELETE FROM agent_spawn_edges WHERE parent_session_id = ? OR child_session_id = ?",
            turso::params![session_id, session_id],
        )
        .await
    {
        log::warn!("failed to cascade-delete agent_spawn_edges for {session_id}: {error}");
    }
    let changed = conn
        .execute("DELETE FROM sessions WHERE id = ?", turso::params![session_id])
        .await
        .map_err(map_err)?;
    if changed == 0 {
        return Err(SessionError::new(
            SessionErrorCode::NotFound,
            format!("Session {session_id} not found"),
        ));
    }
    Ok(())
}

fn row_to_metadata(row: &turso::Row, db_path: &str) -> Result<TursoSessionMetadata, SessionError> {
    let id: String = row.get(0).map_err(map_err)?;
    let created_at: String = row.get(1).map_err(map_err)?;
    let updated_at: String = row.get(2).map_err(map_err).unwrap_or_else(|_| created_at.clone());
    let cwd: Option<String> = row.get(3).map_err(map_err)?;
    let parent_session_id: Option<String> = row.get(4).map_err(map_err)?;
    let provider_id: Option<String> = row.get(5).map_err(map_err)?;
    let model_id: Option<String> = row.get(6).map_err(map_err)?;
    let agent_mode: Option<String> = row.get(7).map_err(map_err)?;
    let name: Option<String> = row.get(8).map_err(map_err)?;
    Ok(TursoSessionMetadata {
        id,
        created_at,
        updated_at,
        cwd: cwd.filter(|s| !s.is_empty()).unwrap_or_default(),
        parent_session_id,
        provider_id,
        model_id,
        agent_mode,
        name,
        db_path: db_path.to_string(),
    })
}

fn map_err(error: impl std::fmt::Display) -> SessionError {
    SessionError::new(SessionErrorCode::Storage, error.to_string())
}
