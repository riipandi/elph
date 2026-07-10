//! Turso-backed LangGraph checkpointer.

mod migrate;
mod read;
mod write;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use turso::{Builder, Connection, Database};

use super::util::{default_db_path, secure_dir, secure_file};

/// Turso-backed checkpointer (langgraph-checkpoint contract).
pub struct TursoCheckpointSaver {
    pub(super) db: Database,
    db_path: PathBuf,
    is_setup: AtomicBool,
}

impl TursoCheckpointSaver {
    /// Open (or create) the checkpoint DB at the given path (or default).
    pub async fn open(db_path: Option<PathBuf>) -> anyhow::Result<Self> {
        let db_path = db_path.unwrap_or_else(default_db_path);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
            secure_dir(parent)?;
        }
        let db = Builder::new_local(db_path.to_string_lossy().as_ref())
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("db open: {e}"))?;
        let saver = Self {
            db,
            db_path: db_path.clone(),
            is_setup: AtomicBool::new(false),
        };
        saver.setup().await?;
        secure_file(&db_path)?;
        Ok(saver)
    }

    pub async fn default() -> anyhow::Result<Self> {
        Self::open(None).await
    }

    pub(super) async fn connection(&self) -> anyhow::Result<Connection> {
        self.db.connect().map_err(|e| anyhow::anyhow!("db connect: {e}"))
    }

    async fn setup(&self) -> anyhow::Result<()> {
        if self.is_setup.load(Ordering::Acquire) {
            return Ok(());
        }
        let conn = self.connection().await?;
        // PRAGMA returns rows — must not run inside execute_batch (Turso/libsql).
        let _ = conn.execute("PRAGMA journal_mode=WAL", ()).await;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS checkpoints (
                thread_id TEXT NOT NULL,
                checkpoint_ns TEXT NOT NULL DEFAULT '',
                checkpoint_id TEXT NOT NULL,
                parent_checkpoint_id TEXT,
                type TEXT,
                checkpoint BLOB,
                metadata BLOB,
                PRIMARY KEY (thread_id, checkpoint_ns, checkpoint_id)
            );
            CREATE TABLE IF NOT EXISTS writes (
                thread_id TEXT NOT NULL,
                checkpoint_ns TEXT NOT NULL DEFAULT '',
                checkpoint_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                idx INTEGER NOT NULL,
                channel TEXT NOT NULL,
                type TEXT,
                value BLOB,
                PRIMARY KEY (thread_id, checkpoint_ns, checkpoint_id, task_id, idx)
            );",
        )
        .await
        .map_err(|e| anyhow::anyhow!("schema: {e}"))?;
        self.is_setup.store(true, Ordering::Release);
        Ok(())
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}
