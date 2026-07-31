//! Persistent parent→child spawn edges.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::datastore::with_conn;

#[derive(Clone)]
pub struct AgentGraphStore {
    db_path: PathBuf,
}

impl AgentGraphStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub async fn record_spawn(
        &self,
        parent_session_id: &str,
        child_session_id: &str,
        agent_path: &str,
        depth: u32,
    ) -> Result<()> {
        with_conn(&self.db_path, |conn| async move {
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
        with_conn(&self.db_path, |conn| async move {
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
        with_conn(&self.db_path, |conn| async move {
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
