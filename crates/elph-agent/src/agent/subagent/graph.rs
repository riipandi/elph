//! Persistent parent→child spawn edges.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use turso::Connection;

use crate::datastore::{connect, with_conn};

#[derive(Clone)]
pub struct AgentGraphStore {
    db_path: PathBuf,
    /// Shared database handle injected by the host. When present, the store
    /// connects from this handle instead of opening `db_path` — the host owns
    /// the open/apply-migrations lifetime.
    database: Option<Arc<turso::Database>>,
}

impl AgentGraphStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            database: None,
        }
    }

    /// Attach a shared, already-open database handle. When set, the store
    /// connects from this handle on each operation instead of opening
    /// [`db_path`] — the host is responsible for opening the database and
    /// applying migrations.
    pub fn with_database(mut self, database: Arc<turso::Database>) -> Self {
        self.database = Some(database);
        self
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    async fn with_conn<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Connection) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        match &self.database {
            Some(db) => {
                let conn = connect(db).await?;
                f(conn).await
            }
            None => with_conn(&self.db_path, f).await,
        }
    }

    pub async fn record_spawn(
        &self,
        parent_session_id: &str,
        child_session_id: &str,
        agent_path: &str,
        depth: u32,
    ) -> Result<()> {
        self.with_conn(|conn| async move {
            conn.execute(
                "INSERT OR REPLACE INTO agent_spawn_edges
                 (parent_session_id, child_session_id, agent_path, depth, status)
                 VALUES (?, ?, ?, ?, 'open')",
                turso::params![parent_session_id, child_session_id, agent_path, depth as i64],
            )
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn close_edge(&self, parent_session_id: &str, child_session_id: &str) -> Result<()> {
        self.with_conn(|conn| async move {
            conn.execute(
                "UPDATE agent_spawn_edges SET status = 'closed'
                 WHERE parent_session_id = ? AND child_session_id = ?",
                turso::params![parent_session_id, child_session_id],
            )
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn list_open_children(&self, parent_session_id: &str) -> Result<Vec<String>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT child_session_id FROM agent_spawn_edges
                     WHERE parent_session_id = ? AND status = 'open'
                     ORDER BY created_at",
                    turso::params![parent_session_id],
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(row.get::<String>(0)?);
            }
            Ok(out)
        })
        .await
    }
}
