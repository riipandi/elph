//! Turso-backed session tree storage (Pi sqlite-node aligned schema).
//!
//! Tables: `sessions`, `session_entries`, `session_sequences`.
//! Host platform DBs also hold `goals` / `agent_spawn_edges` in the same file;
//! this backend never mutates those tables.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::datastore::migrations::run as run_migrations;
use crate::datastore::with_write_transaction;
use crate::session::id::{generate_entry_id, generate_session_id};
use crate::session::migrations::SESSION_TREE_MIGRATIONS;

/// Get a properly configured connection (with busy_timeout + foreign_keys pragmas).
async fn connect_configured(db: &turso::Database) -> Result<turso::Connection, SessionError> {
    crate::datastore::connect(db).await.map_err(map_storage_error)
}
use crate::session::storage_utils::{
    append_to_index, build_index, compute_statistics, create_leaf_entry, find_entries, get_entries_cursor,
    get_path_to_root, get_path_to_root_or_compaction,
};
use crate::session::types::{
    CheckpointTail, CursorPosition, SessionError, SessionErrorCode, SessionIndex, SessionMetadata, SessionStatistics,
    SessionStorage, SessionTreeEntry, TursoSessionMetadata,
};
use turso::Database;

/// Max number of stale `Leaf` entries we tolerate before auto-healing breaks —
/// each stale leaf is proof the tree was written by a crash/partial state.
const MAX_STALE_LEAVES_BEFORE_HEAL: usize = 16;

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
    /// A single reused libSQL connection for this storage instance. libSQL has
    /// no built-in pool, so the old code opened a fresh connection (with its own
    /// page cache + prepared-statement cache) on every tree-entry write / touch,
    /// churning memory and cache under a long session. [`TursoSessionStorage::connection`]
    /// hands out a [`ReusableConn`] that returns the connection to this cache on
    /// drop, so all writes to one session reuse one connection. Clones share the
    /// `Arc`, so concurrent writers serialize on the single connection (correct
    /// for SQLite single-writer).
    conn_cache: Arc<Mutex<Option<turso::Connection>>>,
}

/// A `turso::Connection` handle that returns itself to the storage's connection
/// cache when dropped, so connections are reused instead of re-created per call.
/// Derefs to `turso::Connection`, so existing call sites (`conn.execute`,
/// `with_write_transaction(&conn, ...)`) keep working unchanged.
struct ReusableConn {
    inner: Option<turso::Connection>,
    cache: Arc<Mutex<Option<turso::Connection>>>,
}

impl Deref for ReusableConn {
    type Target = turso::Connection;
    fn deref(&self) -> &turso::Connection {
        self.inner.as_ref().unwrap()
    }
}

impl DerefMut for ReusableConn {
    fn deref_mut(&mut self) -> &mut turso::Connection {
        self.inner.as_mut().unwrap()
    }
}

impl Drop for ReusableConn {
    fn drop(&mut self) {
        if let Some(conn) = self.inner.take() {
            // Return to the cache if it is still empty; otherwise the connection
            // is closed (a concurrent checkout already refilled the cache).
            if let Ok(mut guard) = self.cache.try_lock()
                && guard.is_none()
            {
                *guard = Some(conn);
            }
        }
    }
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
        let database = match database {
            Some(db) => Some(db),
            None => Some(Arc::new(open_db(&db_path).await?)),
        };
        let conn = connect_configured(
            database
                .as_ref()
                .ok_or_else(|| SessionError::new(SessionErrorCode::Storage, "database initialization failed"))?,
        )
        .await?;
        let metadata = load_metadata(&conn, &session_id, &db_path).await?;
        let entries = load_entries(&conn, &session_id).await?;
        // Resolve the leaf with tolerance for stale pointers (crash ordering,
        // rows pruned by snapshot cleanup, partial recovery writes). Failing
        // open here would make every `--continue`/`--resume` unrecoverable.
        let persisted = load_leaf_id(&conn, &session_id).await?;
        let persisted_snapshot = persisted.clone();
        let index = build_index(entries, persisted)?;
        // Sync resolved leaf back to the DB when the persisted pointer was
        // stale/phantom — without this the first write's CAS on
        // `active_leaf_id` would see a mismatch and roll back the transaction.
        if index.leaf_id.as_deref() != persisted_snapshot.as_deref() {
            let updated_at = crate::messages::now_iso_timestamp();
            if let Err(error) = conn
                .execute(
                    "UPDATE sessions SET active_leaf_id = ?, updated_at = ? WHERE id = ?",
                    turso::params![
                        index.leaf_id.as_deref().unwrap_or(""),
                        updated_at.as_str(),
                        session_id.as_str(),
                    ],
                )
                .await
                .map_err(map_storage_error)
            {
                log::warn!(
                    "session {session_id}: resolved phantom leaf in memory but could not persist active_leaf_id update: {error}"
                );
            }
        }
        // Best-effort auto-heal: if the tree is riddled with phantom leaves
        // (>= 16), drop stale `Leaf` entries and re-resolve so a single corrupt
        // row can't keep poisoning the leaf forever. When we heal, ALSO persist
        // the cleanup (delete the stale rows) so the DB is self-consistent and
        // we don't re-heal on every open.
        let (index, stale_ids) = maybe_heal_stale_leaves(index)?;
        if !stale_ids.is_empty() {
            // Best-effort persistence: the in-memory heal already made the session
            // usable; a DB-level delete failure (e.g. another process holds the
            // write lock) must not fail the whole open — we simply re-heal next time.
            if let Err(error) = persist_heal_stale_leaves(&conn, &session_id, &stale_ids).await {
                log::warn!(
                    "session {session_id}: healed {} stale leaf entries in memory but could not persist cleanup: {error}",
                    stale_ids.len()
                );
            } else {
                log::info!("session {session_id}: healed {} stale leaf entries", stale_ids.len());
            }
        }
        Ok(Self {
            db_path,
            session_id,
            metadata,
            index,
            database,
            conn_cache: Arc::new(Mutex::new(None)),
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
        let database = match database {
            Some(db) => db,
            None => Arc::new(open_db(&db_path).await?),
        };
        let conn = connect_configured(&database).await?;
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

        // Serialized WAL transaction: atomic creation, not MVCC.
        with_write_transaction(&conn, || async {
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

            Ok::<(), anyhow::Error>(())
        })
        .await
        .map_err(map_storage_error)?;

        Ok(Self {
            db_path,
            session_id,
            metadata,
            index: build_index(Vec::new(), None)?,
            database: Some(database),
            conn_cache: Arc::new(Mutex::new(None)),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Bump `updated_at` on the `sessions` row (and cached metadata) to now.
    ///
    /// Used to keep the session visible at the top of the resume list and
    /// protected from retention eviction even when no tree entry is appended
    /// (e.g. opening a session with no writes during that visit).
    pub async fn touch(&mut self) -> Result<(), SessionError> {
        let conn = self.connection().await?;
        let now = crate::messages::now_iso_timestamp();
        conn.execute(
            "UPDATE sessions SET updated_at = ? WHERE id = ?",
            turso::params![now.as_str(), self.session_id.as_str()],
        )
        .await
        .map_err(map_storage_error)?;
        self.metadata.updated_at = now;
        Ok(())
    }

    async fn connection(&self) -> Result<ReusableConn, SessionError> {
        // Fast path: reuse a connection already in the cache (no new connection,
        // no fresh page/statement-cache churn).
        if let Some(conn) = self.conn_cache.lock().await.take() {
            return Ok(ReusableConn {
                inner: Some(conn),
                cache: self.conn_cache.clone(),
            });
        }
        // Slow path: connect (no lock held during the async connect).
        let db = match &self.database {
            Some(db) => db.clone(),
            None => Arc::new(open_db(&self.db_path).await?),
        };
        let conn = connect_configured(&db).await?;
        // Hand out the new connection; it returns to `conn_cache` on drop.
        Ok(ReusableConn {
            inner: Some(conn),
            cache: self.conn_cache.clone(),
        })
    }

    async fn persist_leaf_id(
        &self,
        conn: &turso::Connection,
        expected_leaf_id: Option<&str>,
        leaf_id: Option<&str>,
    ) -> Result<(), SessionError> {
        let updated_at = crate::messages::now_iso_timestamp();
        let changed = match expected_leaf_id {
            Some(expected) => conn
                .execute(
                    "UPDATE sessions SET active_leaf_id = ?, updated_at = ?
                     WHERE id = ? AND active_leaf_id = ?",
                    turso::params![leaf_id, updated_at.as_str(), self.session_id.as_str(), expected],
                )
                .await
                .map_err(map_storage_error)?,
            None => conn
                .execute(
                    "UPDATE sessions SET active_leaf_id = ?, updated_at = ?
                     WHERE id = ? AND active_leaf_id IS NULL",
                    turso::params![leaf_id, updated_at.as_str(), self.session_id.as_str()],
                )
                .await
                .map_err(map_storage_error)?,
        };
        if changed == 0 {
            return Err(SessionError::new(
                SessionErrorCode::Conflict,
                format!("session {} changed in another process; reload before writing", self.session_id),
            ));
        }
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
            // Recovery: if session_sequences row is missing (due to previous failed
            // session creation), insert it with next_seq=0 and return 0. This handles
            // the case where a session row exists but session_sequences was never created.
            conn.execute(
                "INSERT INTO session_sequences (session_id, next_seq) VALUES (?, 0)",
                turso::params![self.session_id.as_str()],
            )
            .await
            .map_err(|e| {
                SessionError::new(
                    SessionErrorCode::Storage,
                    format!("failed to recover missing session_sequences for {}: {e}", self.session_id),
                )
            })?;
            return Ok(0);
        };
        row.get::<i64>(0).map_err(map_storage_error)
    }

    async fn persist_entry(&self, conn: &turso::Connection, entry: &SessionTreeEntry) -> Result<(), SessionError> {
        let seq = self.allocate_seq(conn).await?;
        let payload = serde_json::to_string(entry).map_err(map_storage_error)?;
        let payload_bytes = payload.len() as i64;
        let updated_at = crate::messages::now_iso_timestamp();
        conn.execute(
            "INSERT INTO session_entries (
                session_id, id, entry_seq, parent_id, type, timestamp,
                turn_id, role, payload_bytes, payload
             ) VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?)",
            turso::params![
                self.session_id.as_str(),
                entry.id(),
                seq,
                entry.parent_id(),
                entry.entry_type(),
                entry.timestamp(),
                entry.message_role(),
                payload_bytes,
                payload.as_str(),
            ],
        )
        .await
        .map_err(map_storage_error)?;

        // Touch updated_at + size rollups so list ordering and retention budgets stay current.
        conn.execute(
            "UPDATE sessions SET
                updated_at = ?,
                entry_count = entry_count + 1,
                approx_bytes = approx_bytes + ?
             WHERE id = ?",
            turso::params![updated_at.as_str(), payload_bytes, self.session_id.as_str()],
        )
        .await
        .map_err(map_storage_error)?;
        Ok(())
    }

    /// Open one connection and persist `entry` + the new `leaf_id` inside a single
    /// serialized WAL transaction with automatic retry on lock contention.
    /// Using one connection per call keeps the database owner alive through the
    /// entire operation and avoids sequence-allocation races.
    async fn persist_txn(&self, entry: &SessionTreeEntry, leaf_id: Option<&str>) -> Result<(), SessionError> {
        let conn = self.connection().await?;
        let expected_leaf_id = self.index.leaf_id.as_deref();
        with_write_transaction(&conn, || async {
            self.persist_entry(&conn, entry).await?;
            self.persist_leaf_id(&conn, expected_leaf_id, leaf_id).await?;
            Ok::<(), anyhow::Error>(())
        })
        .await
        .map_err(map_storage_error)
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

/// Drop `Leaf` entries whose target no longer exists when the tree is polluted
/// with so many of them that resolve-on-fail would keep guessing wrong.
///
/// Safe: `Leaf` entries are only ever rewritten on `move_to` / `set_leaf_id`, and
/// dropping phantom ones is idempotent — the next `move_to` writes a fresh one.
///
/// Returns the repaired index plus the list of dropped (stale) leaf entry ids so
/// the caller can persist the cleanup (delete those rows) and avoid re-healing
/// on every open.
fn maybe_heal_stale_leaves(index: SessionIndex) -> Result<(SessionIndex, Vec<String>), SessionError> {
    let stale_count = crate::session::storage_utils::stale_leaf_count(&index);
    if stale_count < MAX_STALE_LEAVES_BEFORE_HEAL {
        return Ok((index, Vec::new()));
    }
    log::warn!("session tree has {stale_count} stale leaf entries — dropping them so the leaf can resolve",);
    let stale_ids: Vec<String> = index
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                SessionTreeEntry::Leaf {
                    target_id: Some(target),
                    ..
                } if !index.by_id.contains_key(target)
            )
        })
        .map(|entry| entry.id().to_string())
        .collect();
    let keep: std::collections::HashSet<String> = stale_ids.iter().cloned().collect();
    let entries: Vec<SessionTreeEntry> = index
        .entries
        .iter()
        .filter(|entry| !keep.contains(entry.id()))
        .cloned()
        .collect();
    // Preserve in-memory side state (checkpoints, name) across the rebuild —
    // dropping phantom leaves must not silently lose live compaction checkpoints.
    let mut healed = build_index(entries, None)?;
    healed.checkpoints = index.checkpoints;
    healed.name = index.name;
    Ok((healed, stale_ids))
}

/// Delete the given stale `Leaf` rows from `session_entries` in one serialized WAL transaction.
///
/// Called only after `maybe_heal_stale_leaves` decided to heal (>= threshold).
/// Best-effort but transactional: if the delete fails the open still succeeds
/// (the in-memory heal already made the session usable); we re-heal next open.
async fn persist_heal_stale_leaves(
    conn: &turso::Connection,
    session_id: &str,
    stale_ids: &[String],
) -> Result<(), SessionError> {
    if stale_ids.is_empty() {
        return Ok(());
    }
    with_write_transaction(conn, || async {
        for id in stale_ids {
            conn.execute(
                "DELETE FROM session_entries WHERE session_id = ? AND id = ?",
                turso::params![session_id, id.as_str()],
            )
            .await
            .map_err(|e| SessionError::new(SessionErrorCode::Storage, format!("heal delete {id}: {e}")))?;
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
    .map_err(map_storage_error)
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
            // Leaf pointer resolved to a phantom (crash between leaf-write and
            // child write, rows pruned). Report `None` instead of failing so the
            // harness can append a fresh entry and re-establish a real leaf.
            return Ok(None);
        }
        Ok(self.index.leaf_id.clone())
    }

    async fn set_leaf_id(&mut self, leaf_id: Option<String>) -> Result<(), SessionError> {
        // Contract: pointing the leaf at an entry that isn't in the tree is a
        // genuine error. Callers that may race a broken tree guard first (recovery
        // resolves to a real entry; flush_pending_session_writes skips phantoms).
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

    async fn touch_timestamp(&mut self) -> Result<(), SessionError> {
        self.touch().await
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

    async fn physical_prune_except(&mut self, keep_ids: &[String]) -> Result<usize, SessionError> {
        if keep_ids.is_empty() {
            return Ok(0);
        }
        let keep: std::collections::HashSet<&str> = keep_ids.iter().map(String::as_str).collect();
        let to_delete: Vec<String> = self
            .index
            .entries
            .iter()
            .map(|e| e.id().to_string())
            .filter(|id| !keep.contains(id.as_str()))
            .collect();
        if to_delete.is_empty() {
            return Ok(0);
        }

        let conn = self.connection().await?;
        let (deleted, leaf_id) = with_write_transaction(&conn, || async {
            let mut deleted = 0usize;
            let mut freed_bytes: i64 = 0;
            for id in &to_delete {
                let mut rows = conn
                    .query(
                        "SELECT payload_bytes FROM session_entries WHERE session_id = ? AND id = ?",
                        turso::params![self.session_id.as_str(), id.as_str()],
                    )
                    .await
                    .map_err(map_storage_error)?;
                if let Some(row) = rows.next().await.map_err(map_storage_error)? {
                    let bytes: i64 = row.get(0).unwrap_or(0);
                    freed_bytes += bytes;
                }
                while rows.next().await.map_err(map_storage_error)?.is_some() {}

                conn.execute(
                    "DELETE FROM session_entries WHERE session_id = ? AND id = ?",
                    turso::params![self.session_id.as_str(), id.as_str()],
                )
                .await
                .map_err(map_storage_error)?;
                deleted += 1;
            }
            let remaining: Vec<SessionTreeEntry> = self
                .index
                .entries
                .iter()
                .filter(|e| keep.contains(e.id()))
                .cloned()
                .collect();
            let leaf = self.index.leaf_id.clone().filter(|id| keep.contains(id.as_str()));
            let updated_at = crate::messages::now_iso_timestamp();
            conn.execute(
                "UPDATE sessions SET
                    entry_count = MAX(0, entry_count - ?),
                    approx_bytes = MAX(0, approx_bytes - ?),
                    active_leaf_id = ?,
                    updated_at = ?
                 WHERE id = ?",
                turso::params![
                    deleted as i64,
                    freed_bytes,
                    leaf.as_deref(),
                    updated_at.as_str(),
                    self.session_id.as_str()
                ],
            )
            .await
            .map_err(map_storage_error)?;
            Ok::<(usize, (Vec<SessionTreeEntry>, Option<String>)), anyhow::Error>((deleted, (remaining, leaf)))
        })
        .await
        .map_err(map_storage_error)?;

        let (remaining, leaf) = leaf_id;
        self.index = build_index(remaining, leaf)?;
        log::info!(
            "session {}: physical_prune deleted {deleted} entries (kept {})",
            self.session_id,
            keep_ids.len()
        );
        Ok(deleted)
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
