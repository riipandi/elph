//! Turso-backed session tree storage (Pi sqlite-node aligned schema).
//!
//! Tables: `sessions`, `session_entries`, `session_sequences`.
//! Host platform DBs also hold `goals` / `agent_spawn_edges` in the same file;
//! this backend never mutates those tables.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::datastore::migrations::run as run_migrations;
use crate::session::id::{generate_entry_id, generate_session_id};
use crate::session::migrations::SESSION_TREE_MIGRATIONS;
use crate::session::storage_utils::{
    append_to_index, build_index, compute_statistics, create_leaf_entry, find_entries, get_entries_cursor,
    get_path_to_root, get_path_to_root_or_compaction,
};
use crate::session::types::{
    CheckpointTail, CursorPosition, SessionError, SessionErrorCode, SessionIndex, SessionMetadata, SessionStatistics,
    SessionStorage, SessionTreeEntry, TursoSessionMetadata,
};
use turso::Database;

/// Options when creating a session row in a shared database.
#[derive(Debug, Clone, Default)]
pub struct TursoSessionCreateOptions {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub parent_session_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_mode: Option<String>,
    pub name: Option<String>,
    pub system_prompt: Option<String>,
    pub metadata_json: Option<String>,
}

#[derive(Clone)]
pub struct TursoSessionStorage {
    db_path: PathBuf,
    session_id: String,
    metadata: TursoSessionMetadata,
    index: SessionIndex,
    /// Shared database handle injected by the host. When present, the storage
    /// connects from this handle instead of opening `db_path` — the host owns
    /// the open/apply-migrations lifetime.
    database: Option<Arc<Database>>,
}

impl TursoSessionStorage {
    /// Open a session, loading its metadata and entry tree.
    ///
    /// When `database` is supplied, the storage connects from that shared
    /// handle (the host must have applied the session-tree migrations already);
    /// otherwise it opens `db_path` and applies migrations itself.
    pub async fn open(
        db_path: impl AsRef<Path>,
        session_id: impl Into<String>,
        database: Option<Arc<Database>>,
    ) -> Result<Self, SessionError> {
        let db_path = db_path.as_ref().to_path_buf();
        let session_id = session_id.into();
        let conn = match &database {
            Some(db) => db.connect().map_err(map_storage_error)?,
            None => {
                let db = open_db(&db_path).await?;
                db.connect().map_err(map_storage_error)?
            }
        };
        let metadata = load_metadata(&conn, &session_id, &db_path).await?;
        let entries = load_entries(&conn, &session_id).await?;
        let leaf_id = load_leaf_id(&conn, &session_id).await?;
        let index = build_index(entries, leaf_id)?;
        Ok(Self {
            db_path,
            session_id,
            metadata,
            index,
            database,
        })
    }

    pub async fn create(db_path: impl AsRef<Path>, session_id: Option<String>) -> Result<Self, SessionError> {
        Self::create_with_options_with_db(
            db_path,
            TursoSessionCreateOptions {
                session_id,
                ..Default::default()
            },
            None,
        )
        .await
    }

    pub async fn create_with_options(
        db_path: impl AsRef<Path>,
        options: TursoSessionCreateOptions,
    ) -> Result<Self, SessionError> {
        Self::create_with_options_with_db(db_path, options, None).await
    }

    /// Create a session, optionally using a shared, already-open database
    /// handle. When `database` is `None`, `db_path` is opened and the
    /// session-tree migrations are applied.
    pub async fn create_with_options_with_db(
        db_path: impl AsRef<Path>,
        options: TursoSessionCreateOptions,
        database: Option<Arc<Database>>,
    ) -> Result<Self, SessionError> {
        let db_path = db_path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(map_storage_error)?;
        }
        let session_id = options.session_id.unwrap_or_else(generate_session_id);
        let conn = match &database {
            Some(db) => db.connect().map_err(map_storage_error)?,
            None => {
                let db = open_db(&db_path).await?;
                db.connect().map_err(map_storage_error)?
            }
        };
        let created_at = crate::messages::now_iso_timestamp();
        let cwd = options.cwd.unwrap_or_default();
        let agent_mode = options.agent_mode.unwrap_or_else(|| "build".to_string());
        let metadata = TursoSessionMetadata {
            id: session_id.clone(),
            created_at: created_at.clone(),
            updated_at: created_at.clone(),
            cwd: cwd.clone(),
            parent_session_id: options.parent_session_id.clone(),
            provider_id: options.provider_id.clone(),
            model_id: options.model_id.clone(),
            agent_mode: Some(agent_mode.clone()),
            name: options.name.clone(),
            db_path: db_path.to_string_lossy().to_string(),
        };

        conn.execute(
            "INSERT INTO sessions (
                id, created_at, updated_at, cwd, parent_session_id,
                provider_id, model_id, agent_mode, name, system_prompt, metadata, active_leaf_id
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
            turso::params![
                session_id.as_str(),
                created_at.as_str(),
                created_at.as_str(),
                cwd.as_str(),
                options.parent_session_id.as_deref(),
                options.provider_id.as_deref(),
                options.model_id.as_deref(),
                agent_mode.as_str(),
                options.name.as_deref(),
                options.system_prompt.as_deref(),
                options.metadata_json.as_deref(),
            ],
        )
        .await
        .map_err(map_storage_error)?;

        conn.execute(
            "INSERT INTO session_sequences (session_id, next_seq) VALUES (?, 0)",
            turso::params![session_id.as_str()],
        )
        .await
        .map_err(map_storage_error)?;

        Ok(Self {
            db_path,
            session_id,
            metadata,
            index: build_index(Vec::new(), None)?,
            database,
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn connection(&self) -> Result<turso::Connection, SessionError> {
        match &self.database {
            Some(db) => db.connect().map_err(map_storage_error),
            None => {
                let db = open_db(&self.db_path).await?;
                db.connect().map_err(map_storage_error)
            }
        }
    }

    async fn persist_leaf_id(&self, conn: &turso::Connection, leaf_id: Option<&str>) -> Result<(), SessionError> {
        let updated_at = crate::messages::now_iso_timestamp();
        conn.execute(
            "UPDATE sessions SET active_leaf_id = ?, updated_at = ? WHERE id = ?",
            turso::params![leaf_id, updated_at.as_str(), self.session_id.as_str()],
        )
        .await
        .map_err(map_storage_error)?;
        Ok(())
    }

    async fn allocate_seq(&self, conn: &turso::Connection) -> Result<i64, SessionError> {
        // Atomic read-modify-write: bump `next_seq` and return the new value in a
        // single statement so two concurrent writers cannot observe the same seq.
        // The caller wraps this in a `BEGIN IMMEDIATE` transaction for full isolation.
        let mut rows = conn
            .query(
                "UPDATE session_sequences SET next_seq = next_seq + 1
                 WHERE session_id = ? RETURNING next_seq",
                turso::params![self.session_id.as_str()],
            )
            .await
            .map_err(map_storage_error)?;
        let Some(row) = rows.next().await.map_err(map_storage_error)? else {
            return Err(SessionError::new(
                SessionErrorCode::InvalidSession,
                format!("session_sequences missing for {}", self.session_id),
            ));
        };
        row.get::<i64>(0).map_err(map_storage_error)
    }

    async fn persist_entry(&self, conn: &turso::Connection, entry: &SessionTreeEntry) -> Result<(), SessionError> {
        let seq = self.allocate_seq(conn).await?;
        let payload = serde_json::to_string(entry).map_err(map_storage_error)?;
        let updated_at = crate::messages::now_iso_timestamp();
        conn.execute(
            "INSERT INTO session_entries (
                session_id, id, entry_seq, parent_id, type, timestamp, payload
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
            turso::params![
                self.session_id.as_str(),
                entry.id(),
                seq,
                entry.parent_id(),
                entry.entry_type(),
                entry.timestamp(),
                payload.as_str(),
            ],
        )
        .await
        .map_err(map_storage_error)?;

        // Touch updated_at so list ordering advances even when the leaf is unchanged.
        conn.execute(
            "UPDATE sessions SET updated_at = ? WHERE id = ?",
            turso::params![updated_at.as_str(), self.session_id.as_str()],
        )
        .await
        .map_err(map_storage_error)?;
        Ok(())
    }

    /// Open one connection and persist `entry` + the new `leaf_id` inside a single
    /// `BEGIN IMMEDIATE` … `COMMIT` transaction, rolling back on error. Using one
    /// connection per call (instead of one per write) avoids re-opening the
    /// multiprocess-WAL database on every append and removes the sequence-allocation
    /// race under concurrent, multi-process writers.
    async fn persist_txn(&self, entry: &SessionTreeEntry, leaf_id: Option<&str>) -> Result<(), SessionError> {
        let conn = self.connection().await?;
        conn.execute("BEGIN IMMEDIATE", ()).await.map_err(map_storage_error)?;
        let outcome = async {
            self.persist_entry(&conn, entry).await?;
            self.persist_leaf_id(&conn, leaf_id).await?;
            Ok::<(), SessionError>(())
        }
        .await;
        match outcome {
            Ok(()) => {
                conn.execute("COMMIT", ()).await.map_err(map_storage_error)?;
                Ok(())
            }
            Err(error) => {
                let _ = conn.execute("ROLLBACK", ()).await;
                Err(error)
            }
        }
    }
}

async fn open_db(path: &Path) -> Result<turso::Database, SessionError> {
    let db = crate::datastore::open_local(path).await.map_err(map_storage_error)?;
    let conn = crate::datastore::connect(&db).await.map_err(map_storage_error)?;
    run_migrations(&conn, &SESSION_TREE_MIGRATIONS)
        .await
        .map_err(|error| SessionError::new(SessionErrorCode::Storage, error.to_string()))?;
    Ok(db)
}

async fn load_metadata(
    conn: &turso::Connection,
    session_id: &str,
    db_path: &Path,
) -> Result<TursoSessionMetadata, SessionError> {
    let mut rows = conn
        .query(
            "SELECT created_at, updated_at, cwd, parent_session_id,
                    provider_id, model_id, agent_mode, name
             FROM sessions WHERE id = ?",
            turso::params![session_id],
        )
        .await
        .map_err(map_storage_error)?;
    let Some(row) = rows.next().await.map_err(map_storage_error)? else {
        return Err(SessionError::new(
            SessionErrorCode::NotFound,
            format!("Session {session_id} not found"),
        ));
    };
    let created_at: String = row.get(0).map_err(map_storage_error)?;
    let updated_at: String = row
        .get(1)
        .map_err(map_storage_error)
        .unwrap_or_else(|_| created_at.clone());
    let cwd: Option<String> = row.get(2).map_err(map_storage_error)?;
    let parent_session_id: Option<String> = row.get(3).map_err(map_storage_error)?;
    let provider_id: Option<String> = row.get(4).map_err(map_storage_error)?;
    let model_id: Option<String> = row.get(5).map_err(map_storage_error)?;
    let agent_mode: Option<String> = row.get(6).map_err(map_storage_error)?;
    let name: Option<String> = row.get(7).map_err(map_storage_error)?;
    while rows.next().await.map_err(map_storage_error)?.is_some() {}

    let cwd = cwd.filter(|s| !s.is_empty()).unwrap_or_default();

    Ok(TursoSessionMetadata {
        id: session_id.to_string(),
        created_at,
        updated_at,
        cwd,
        parent_session_id,
        provider_id,
        model_id,
        agent_mode,
        name,
        db_path: db_path.to_string_lossy().to_string(),
    })
}

async fn load_leaf_id(conn: &turso::Connection, session_id: &str) -> Result<Option<String>, SessionError> {
    let mut rows = conn
        .query("SELECT active_leaf_id FROM sessions WHERE id = ?", turso::params![session_id])
        .await
        .map_err(map_storage_error)?;
    let leaf_id = if let Some(row) = rows.next().await.map_err(map_storage_error)? {
        row.get::<Option<String>>(0).map_err(map_storage_error)?
    } else {
        None
    };
    while rows.next().await.map_err(map_storage_error)?.is_some() {}
    Ok(leaf_id)
}

async fn load_entries(conn: &turso::Connection, session_id: &str) -> Result<Vec<SessionTreeEntry>, SessionError> {
    let mut rows = conn
        .query(
            "SELECT payload FROM session_entries WHERE session_id = ? ORDER BY entry_seq ASC",
            turso::params![session_id],
        )
        .await
        .map_err(map_storage_error)?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(map_storage_error)? {
        let data: String = row.get(0).map_err(map_storage_error)?;
        let entry: SessionTreeEntry = serde_json::from_str(&data).map_err(map_storage_error)?;
        entries.push(entry);
    }
    Ok(entries)
}

fn map_storage_error(error: impl std::fmt::Display) -> SessionError {
    SessionError::new(SessionErrorCode::Storage, error.to_string())
}

impl SessionStorage for TursoSessionStorage {
    type Metadata = TursoSessionMetadata;

    async fn get_metadata(&self) -> Self::Metadata {
        self.metadata.clone()
    }

    async fn get_leaf_id(&self) -> Result<Option<String>, SessionError> {
        if let Some(leaf_id) = &self.index.leaf_id
            && !self.index.by_id.contains_key(leaf_id)
        {
            return Err(SessionError::new(
                SessionErrorCode::InvalidSession,
                format!("Entry {leaf_id} not found"),
            ));
        }
        Ok(self.index.leaf_id.clone())
    }

    async fn set_leaf_id(&mut self, leaf_id: Option<String>) -> Result<(), SessionError> {
        if let Some(leaf_id) = &leaf_id
            && !self.index.by_id.contains_key(leaf_id)
        {
            return Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Entry {leaf_id} not found"),
            ));
        }
        let entry = create_leaf_entry(self.index.leaf_id.clone(), leaf_id.clone(), &self.index.by_id);
        self.persist_txn(&entry, leaf_id.as_deref()).await?;
        append_to_index(&mut self.index, entry);
        Ok(())
    }

    async fn create_entry_id(&self) -> String {
        generate_entry_id(&self.index.by_id)
    }

    async fn append_entry(&mut self, entry: SessionTreeEntry) -> Result<(), SessionError> {
        // The appended entry becomes the new leaf.
        self.persist_txn(&entry, Some(entry.id())).await?;
        append_to_index(&mut self.index, entry);
        // Refresh updated_at in cached metadata for callers of get_metadata.
        self.metadata.updated_at = crate::messages::now_iso_timestamp();
        Ok(())
    }

    async fn get_entry(&self, id: &str) -> Option<SessionTreeEntry> {
        self.index.by_id.get(id).cloned()
    }

    async fn find_entries(&self, entry_type: &str) -> Vec<SessionTreeEntry> {
        find_entries(&self.index.entries, entry_type)
    }

    async fn get_label(&self, id: &str) -> Option<String> {
        self.index.labels_by_id.get(id).cloned()
    }

    async fn get_path_to_root(&self, leaf_id: Option<&str>) -> Result<Vec<SessionTreeEntry>, SessionError> {
        get_path_to_root(&self.index.by_id, leaf_id)
    }

    async fn get_path_to_root_or_compaction(
        &self,
        leaf_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionError> {
        get_path_to_root_or_compaction(&self.index.by_id, leaf_id)
    }

    async fn get_entries(&self) -> Vec<SessionTreeEntry> {
        self.index.entries.clone()
    }

    async fn get_entries_cursor(&self, cursor: &CursorPosition) -> Result<Vec<SessionTreeEntry>, SessionError> {
        get_entries_cursor(&self.index.entries, cursor)
    }

    async fn get_statistics(&self) -> SessionStatistics {
        compute_statistics(&self.index)
    }

    async fn store_checkpoint_tail(&mut self, tail: CheckpointTail) -> Result<String, SessionError> {
        let root_id = tail.root_id.clone();
        self.index.checkpoints.insert(root_id.clone(), tail);
        Ok(root_id)
    }

    async fn load_checkpoint_tail(&self, root_id: &str) -> Result<Option<CheckpointTail>, SessionError> {
        Ok(self.index.checkpoints.get(root_id).cloned())
    }

    async fn list_checkpoint_tails(&self) -> Vec<String> {
        self.index.checkpoints.keys().cloned().collect()
    }

    async fn get_name(&self) -> Option<String> {
        self.index.name.clone().or_else(|| self.metadata.name.clone())
    }
}

impl From<TursoSessionMetadata> for SessionMetadata {
    fn from(value: TursoSessionMetadata) -> Self {
        Self {
            id: value.id,
            created_at: value.created_at,
        }
    }
}
