//! Turso-backed session tree storage.

use std::path::{Path, PathBuf};

use turso::Builder;

use crate::datastore::migrations::run as run_migrations;
use crate::session::id::{generate_entry_id, generate_session_id};
use crate::session::migrations::SESSION_TREE_MIGRATIONS;
use crate::session::storage_utils::{append_to_index, build_index, create_leaf_entry, find_entries, get_path_to_root};
use crate::session::types::{
    SessionError, SessionErrorCode, SessionIndex, SessionMetadata, SessionStorage, SessionTreeEntry,
    TursoSessionMetadata,
};

pub struct TursoSessionStorage {
    db_path: PathBuf,
    session_id: String,
    metadata: TursoSessionMetadata,
    index: SessionIndex,
}

impl TursoSessionStorage {
    pub async fn open(db_path: impl AsRef<Path>, session_id: impl Into<String>) -> Result<Self, SessionError> {
        let db_path = db_path.as_ref().to_path_buf();
        let session_id = session_id.into();
        let db = open_db(&db_path).await?;
        let conn = db.connect().map_err(map_storage_error)?;
        let metadata = load_metadata(&conn, &session_id, &db_path).await?;
        let entries = load_entries(&conn, &session_id).await?;
        let leaf_id = load_leaf_id(&conn, &session_id).await?;
        let index = build_index(entries, leaf_id)?;
        Ok(Self {
            db_path,
            session_id,
            metadata,
            index,
        })
    }

    pub async fn create(db_path: impl AsRef<Path>, session_id: Option<String>) -> Result<Self, SessionError> {
        let db_path = db_path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(map_storage_error)?;
        }
        let session_id = session_id.unwrap_or_else(generate_session_id);
        let db = open_db(&db_path).await?;
        let conn = db.connect().map_err(map_storage_error)?;
        let created_at = crate::messages::now_iso_timestamp();
        let metadata = TursoSessionMetadata {
            id: session_id.clone(),
            created_at: created_at.clone(),
            db_path: db_path.to_string_lossy().to_string(),
        };
        let metadata_json = serde_json::to_string(&metadata).map_err(map_storage_error)?;
        conn.execute(
            "INSERT INTO session_meta (session_id, leaf_id, created_at, metadata)
             VALUES (?, NULL, ?, ?)",
            turso::params![session_id.as_str(), created_at.as_str(), metadata_json.as_str()],
        )
        .await
        .map_err(map_storage_error)?;
        Ok(Self {
            db_path,
            session_id,
            metadata,
            index: build_index(Vec::new(), None)?,
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    async fn connection(&self) -> Result<turso::Connection, SessionError> {
        let db = open_db(&self.db_path).await?;
        db.connect().map_err(map_storage_error)
    }

    async fn persist_leaf_id(&self, leaf_id: Option<&str>) -> Result<(), SessionError> {
        let conn = self.connection().await?;
        conn.execute(
            "UPDATE session_meta SET leaf_id = ? WHERE session_id = ?",
            turso::params![leaf_id, self.session_id.as_str()],
        )
        .await
        .map_err(map_storage_error)?;
        Ok(())
    }

    async fn persist_entry(&self, seq: i64, entry: &SessionTreeEntry) -> Result<(), SessionError> {
        let conn = self.connection().await?;
        let data = serde_json::to_string(entry).map_err(map_storage_error)?;
        conn.execute(
            "INSERT INTO session_tree_entries (session_id, seq, id, data) VALUES (?, ?, ?, ?)",
            turso::params![self.session_id.as_str(), seq, entry.id(), data.as_str()],
        )
        .await
        .map_err(map_storage_error)?;
        Ok(())
    }
}

async fn open_db(path: &Path) -> Result<turso::Database, SessionError> {
    let db = Builder::new_local(path.to_string_lossy().as_ref())
        .build()
        .await
        .map_err(map_storage_error)?;
    let conn = db.connect().map_err(map_storage_error)?;
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
            "SELECT created_at, metadata FROM session_meta WHERE session_id = ?",
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
    let metadata_json: String = row.get(1).map_err(map_storage_error)?;
    while rows.next().await.map_err(map_storage_error)?.is_some() {}
    serde_json::from_str(&metadata_json)
        .map_err(map_storage_error)
        .or_else(|_| {
            Ok(TursoSessionMetadata {
                id: session_id.to_string(),
                created_at,
                db_path: db_path.to_string_lossy().to_string(),
            })
        })
}

async fn load_leaf_id(conn: &turso::Connection, session_id: &str) -> Result<Option<String>, SessionError> {
    let mut rows = conn
        .query(
            "SELECT leaf_id FROM session_meta WHERE session_id = ?",
            turso::params![session_id],
        )
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
            "SELECT data FROM session_tree_entries WHERE session_id = ? ORDER BY seq ASC",
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
        let seq = self.index.entries.len() as i64;
        self.persist_entry(seq, &entry).await?;
        append_to_index(&mut self.index, entry);
        self.persist_leaf_id(self.index.leaf_id.as_deref()).await?;
        Ok(())
    }

    async fn create_entry_id(&self) -> String {
        generate_entry_id(&self.index.by_id)
    }

    async fn append_entry(&mut self, entry: SessionTreeEntry) -> Result<(), SessionError> {
        let seq = self.index.entries.len() as i64;
        self.persist_entry(seq, &entry).await?;
        append_to_index(&mut self.index, entry);
        self.persist_leaf_id(self.index.leaf_id.as_deref()).await?;
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

    async fn get_entries(&self) -> Vec<SessionTreeEntry> {
        self.index.entries.clone()
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
