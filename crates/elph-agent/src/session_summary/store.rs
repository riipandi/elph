//! Turso-backed session summary persistence.
//!
//! One row per session, upserted on compaction. Read by the
//! `get_session_summary` agent tool and by the host to recall past context.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use turso::Connection;

use crate::datastore::{connect, is_lock_err, with_conn};

use super::types::SessionSummary;

const SUMMARY_COLUMNS: &str = "session_id, summary, tokens_before, compaction_count,
    first_kept_entry_id, details, created_at, updated_at";

#[derive(Clone)]
pub struct SessionSummaryStore {
    db_path: PathBuf,
    /// Shared database handle injected by the host. When present, the store
    /// connects from this handle instead of opening `db_path`.
    database: Option<Arc<turso::Database>>,
}

impl SessionSummaryStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            database: None,
        }
    }

    /// Attach a shared, already-open database handle.
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
            None => with_conn(&self.db_path, f)
                .await
                .with_context(|| format!("open session_summary database {}", self.db_path.display())),
        }
    }

    /// Get the summary for a session, if one exists.
    pub async fn get(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    &format!(
                        "SELECT {SUMMARY_COLUMNS} FROM session_summaries
                         WHERE session_id = ?"
                    ),
                    turso::params![session_id],
                )
                .await?;
            if let Some(row) = rows.next().await? {
                return Ok(Some(row_to_summary(&row)?));
            }
            Ok(None)
        })
        .await
    }

    /// Insert or replace the summary for a session. Called after compaction.
    ///
    /// `compaction_count` is auto-incremented from the previous value (or starts
    /// at 1 for a new row) by the SQL upsert — callers do not supply it.
    pub async fn upsert(
        &self,
        session_id: &str,
        summary: &str,
        tokens_before: i64,
        first_kept_entry_id: Option<&str>,
        details: Option<&str>,
    ) -> Result<SessionSummary> {
        if summary.trim().is_empty() {
            bail!("summary must not be empty");
        }

        self.with_conn(|conn| async move {
            conn.execute(
                "INSERT INTO session_summaries (
                    session_id, summary, tokens_before, compaction_count,
                    first_kept_entry_id, details, created_at, updated_at
                 ) VALUES (?, ?, ?, 1, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                 ON CONFLICT(session_id) DO UPDATE SET
                    summary = excluded.summary,
                    tokens_before = excluded.tokens_before,
                    compaction_count = compaction_count + 1,
                    first_kept_entry_id = excluded.first_kept_entry_id,
                    details = excluded.details,
                    updated_at = CURRENT_TIMESTAMP",
                turso::params![session_id, summary, tokens_before, first_kept_entry_id, details,],
            )
            .await?;
            Ok(())
        })
        .await?;

        self.get(session_id)
            .await?
            .context("session_summary upserted but not found")
    }

    /// Delete the summary for a session (e.g. on session delete). Cascade-delete
    /// in DDL usually handles this, but this allows explicit cleanup.
    pub async fn delete(&self, session_id: &str) -> Result<()> {
        self.with_conn(|conn| async move {
            conn.execute("DELETE FROM session_summaries WHERE session_id = ?", turso::params![session_id])
                .await?;
            Ok(())
        })
        .await
    }

    /// Best-effort upsert that swallows lock errors. Used from compaction hooks
    /// where a failed write should not block the compaction flow.
    pub async fn upsert_best_effort(
        &self,
        session_id: &str,
        summary: &str,
        tokens_before: i64,
        first_kept_entry_id: Option<&str>,
        details: Option<&str>,
    ) {
        if summary.trim().is_empty() {
            return;
        }
        match self
            .upsert(session_id, summary, tokens_before, first_kept_entry_id, details)
            .await
        {
            Ok(_) => {}
            Err(err) if is_lock_err(&err.to_string()) => {
                log::warn!("session_summary upsert skipped (database locked): {err}");
            }
            Err(err) => {
                log::warn!("session_summary upsert failed: {err:#}");
            }
        }
    }
}

fn row_to_summary(row: &turso::Row) -> Result<SessionSummary> {
    Ok(SessionSummary {
        session_id: row.get(0)?,
        summary: row.get(1)?,
        tokens_before: row.get(2)?,
        compaction_count: row.get(3)?,
        first_kept_entry_id: row.get(4)?,
        details: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_store() -> (tempfile::TempDir, SessionSummaryStore) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("test.db");
        let store = SessionSummaryStore::new(db_path.clone());

        // Initialize database with the session_summaries table.
        crate::datastore::with_conn(&db_path, |conn| async move {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS session_summaries (
                    session_id TEXT PRIMARY KEY NOT NULL,
                    summary TEXT NOT NULL,
                    tokens_before INTEGER NOT NULL DEFAULT 0,
                    compaction_count INTEGER NOT NULL DEFAULT 0,
                    first_kept_entry_id TEXT,
                    details TEXT,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                ) STRICT",
                (),
            )
            .await
            .map_err(|e| anyhow::anyhow!("create session_summaries table: {e}"))
        })
        .await
        .expect("init db");

        (tmp, store)
    }

    #[tokio::test]
    async fn upsert_inserts_and_updates() {
        let (_tmp, store) = setup_store().await;

        // Insert — compaction_count starts at 1
        let s1 = store
            .upsert("sess_a", "first summary", 1000, Some("entry1"), None)
            .await
            .expect("upsert 1");
        assert_eq!(s1.session_id, "sess_a");
        assert_eq!(s1.summary, "first summary");
        assert_eq!(s1.tokens_before, 1000);
        assert_eq!(s1.compaction_count, 1);
        assert_eq!(s1.first_kept_entry_id.as_deref(), Some("entry1"));

        // Update (same session_id) — compaction_count auto-increments to 2
        let s2 = store
            .upsert("sess_a", "second summary", 2000, Some("entry2"), Some("{}"))
            .await
            .expect("upsert 2");
        assert_eq!(s2.summary, "second summary");
        assert_eq!(s2.tokens_before, 2000);
        assert_eq!(s2.compaction_count, 2);
        assert_eq!(s2.first_kept_entry_id.as_deref(), Some("entry2"));
        assert_eq!(s2.details.as_deref(), Some("{}"));

        // created_at should stay the same, updated_at may differ
        assert_eq!(s1.created_at, s2.created_at);
    }

    #[tokio::test]
    async fn get_returns_none_when_missing() {
        let (_tmp, store) = setup_store().await;
        let result = store.get("sess_missing").await.expect("get");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_removes_row() {
        let (_tmp, store) = setup_store().await;
        store
            .upsert("sess_del", "to be deleted", 500, None, None)
            .await
            .expect("upsert");
        assert!(store.get("sess_del").await.expect("get").is_some());

        store.delete("sess_del").await.expect("delete");
        assert!(store.get("sess_del").await.expect("get").is_none());
    }

    #[tokio::test]
    async fn upsert_rejects_empty_summary() {
        let (_tmp, store) = setup_store().await;
        let result = store.upsert("sess_empty", "   ", 0, None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn upsert_best_effort_swallows_errors() {
        let (_tmp, store) = setup_store().await;
        // Empty summary — silently skipped.
        store.upsert_best_effort("sess_bf", "   ", 0, None, None).await;
        assert!(store.get("sess_bf").await.expect("get").is_none());

        // Valid upsert.
        store
            .upsert_best_effort("sess_bf", "best effort", 100, None, None)
            .await;
        assert!(store.get("sess_bf").await.expect("get").is_some());
    }
}
