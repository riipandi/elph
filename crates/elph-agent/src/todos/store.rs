//! Turso-backed session todo persistence.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use turso::Connection;

use crate::datastore::{connect, with_conn};
use crate::messages::now_iso_timestamp;
use crate::session::id::create_todo_id;

use super::types::{TodoItem, TodoStatus};

const TODO_COLUMNS: &str = "id, session_id, content, status, position, created_at, updated_at, completed_at";

#[derive(Clone)]
pub struct TodoStore {
    db_path: PathBuf,
    database: Option<Arc<turso::Database>>,
}

impl TodoStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            database: None,
        }
    }

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
                .with_context(|| format!("open todo database {}", self.db_path.display())),
        }
    }

    pub async fn list(&self, session_id: &str) -> Result<Vec<TodoItem>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    &format!(
                        "SELECT {TODO_COLUMNS} FROM session_todos
                         WHERE session_id = ?
                         ORDER BY position ASC, created_at ASC"
                    ),
                    turso::params![session_id],
                )
                .await?;
            let mut items = Vec::new();
            while let Some(row) = rows.next().await? {
                items.push(row_to_todo(&row)?);
            }
            Ok(items)
        })
        .await
    }

    /// Replace the entire todo list for a session.
    pub async fn replace(&self, session_id: &str, items: Vec<TodoUpdate>) -> Result<Vec<TodoItem>> {
        validate_updates(&items)?;
        let now = now_iso_timestamp();
        self.with_conn(|conn| async move {
            conn.execute("BEGIN IMMEDIATE", ()).await?;
            let outcome = async {
                conn.execute("DELETE FROM session_todos WHERE session_id = ?", turso::params![session_id])
                    .await?;
                for (position, update) in items.into_iter().enumerate() {
                    insert_todo(&conn, session_id, &update, position as i64, &now).await?;
                }
                Ok::<(), anyhow::Error>(())
            }
            .await;
            match outcome {
                Ok(()) => {
                    conn.execute("COMMIT", ()).await?;
                }
                Err(err) => {
                    let _ = conn.execute("ROLLBACK", ()).await;
                    return Err(err);
                }
            }
            Ok(())
        })
        .await?;
        self.list(session_id).await
    }

    /// Merge updates by id (default agent write path).
    pub async fn merge(&self, session_id: &str, updates: Vec<TodoUpdate>) -> Result<Vec<TodoItem>> {
        validate_updates(&updates)?;
        let now = now_iso_timestamp();
        let existing = self.list(session_id).await?;
        let mut by_id: std::collections::HashMap<String, TodoItem> =
            existing.into_iter().map(|t| (t.id.clone(), t)).collect();

        for update in updates {
            let id = update.id.clone().unwrap_or_else(create_todo_id);
            if let Some(item) = by_id.get_mut(&id) {
                if let Some(content) = update.content {
                    item.content = content;
                }
                if let Some(status) = update.status {
                    item.status = status;
                    item.completed_at = if status == TodoStatus::Completed || status == TodoStatus::Cancelled {
                        Some(now.clone())
                    } else {
                        None
                    };
                }
                item.updated_at = now.clone();
            } else {
                let status = update.status.unwrap_or(TodoStatus::Pending);
                let content = update.content.clone().unwrap_or_else(|| id.clone());
                let completed_at = if status == TodoStatus::Completed || status == TodoStatus::Cancelled {
                    Some(now.clone())
                } else {
                    None
                };
                by_id.insert(
                    id.clone(),
                    TodoItem {
                        id,
                        session_id: session_id.to_string(),
                        content,
                        status,
                        position: by_id.len() as i64,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                        completed_at,
                    },
                );
            }
        }

        // At most one in_progress after merge.
        let in_progress: Vec<_> = by_id
            .values()
            .filter(|t| t.status == TodoStatus::InProgress)
            .map(|t| t.id.clone())
            .collect();
        if in_progress.len() > 1 {
            // Keep the last updated as in_progress; demote others to pending.
            let keep = in_progress.last().cloned();
            for item in by_id.values_mut() {
                if item.status == TodoStatus::InProgress && Some(item.id.as_str()) != keep.as_deref() {
                    item.status = TodoStatus::Pending;
                    item.completed_at = None;
                    item.updated_at = now.clone();
                }
            }
        }

        let mut ordered: Vec<TodoItem> = by_id.into_values().collect();
        ordered.sort_by_key(|t| t.position);
        for (i, item) in ordered.iter_mut().enumerate() {
            item.position = i as i64;
        }

        let replace_items: Vec<TodoUpdate> = ordered
            .iter()
            .map(|t| TodoUpdate {
                id: Some(t.id.clone()),
                content: Some(t.content.clone()),
                status: Some(t.status),
            })
            .collect();
        self.replace(session_id, replace_items).await
    }

    pub async fn clear(&self, session_id: &str) -> Result<()> {
        self.with_conn(|conn| async move {
            conn.execute("DELETE FROM session_todos WHERE session_id = ?", turso::params![session_id])
                .await?;
            Ok(())
        })
        .await
    }
}

/// Partial update used by merge/replace.
#[derive(Debug, Clone)]
pub struct TodoUpdate {
    pub id: Option<String>,
    pub content: Option<String>,
    pub status: Option<TodoStatus>,
}

fn validate_updates(updates: &[TodoUpdate]) -> Result<()> {
    let mut seen = HashSet::new();
    for u in updates {
        if let Some(id) = &u.id
            && !seen.insert(id.clone())
        {
            bail!("duplicate todo id in one call: {id}");
        }
    }
    let in_progress = updates
        .iter()
        .filter(|u| u.status == Some(TodoStatus::InProgress))
        .count();
    if in_progress > 1 {
        bail!("at most one todo may be in_progress per write");
    }
    Ok(())
}

async fn insert_todo(conn: &Connection, session_id: &str, update: &TodoUpdate, position: i64, now: &str) -> Result<()> {
    let id = update.id.clone().unwrap_or_else(create_todo_id);
    let content = update.content.clone().unwrap_or_else(|| id.clone());
    let status = update.status.unwrap_or(TodoStatus::Pending);
    let completed_at = if matches!(status, TodoStatus::Completed | TodoStatus::Cancelled) {
        Some(now)
    } else {
        None
    };
    conn.execute(
        "INSERT INTO session_todos (
            id, session_id, content, status, position, created_at, updated_at, completed_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        turso::params![
            id.as_str(),
            session_id,
            content.as_str(),
            status.as_str(),
            position,
            now,
            now,
            completed_at,
        ],
    )
    .await?;
    Ok(())
}

fn row_to_todo(row: &turso::Row) -> Result<TodoItem> {
    let id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let content: String = row.get(2)?;
    let status_s: String = row.get(3)?;
    let position: i64 = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;
    let completed_at: Option<String> = row.get(7)?;
    let status = TodoStatus::parse(&status_s).unwrap_or(TodoStatus::Pending);
    Ok(TodoItem {
        id,
        session_id,
        content,
        status,
        position,
        created_at,
        updated_at,
        completed_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::ensure_database;
    use crate::session::migrations::SESSION_TREE_MIGRATIONS;

    async fn setup() -> (tempfile::TempDir, TodoStore, String) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("store.db");
        ensure_database(&db, &SESSION_TREE_MIGRATIONS).await.expect("migrate");
        // sessions row required only if FK were enforced; schema has no FK constraint for simplicity.
        let conn = crate::datastore::open_local(&db).await.expect("open");
        let c = crate::datastore::connect(&conn).await.expect("connect");
        let sid = "sess_todo_test";
        c.execute(
            "INSERT INTO sessions (id, created_at, updated_at, cwd) VALUES (?, ?, ?, ?)",
            turso::params![sid, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "/tmp"],
        )
        .await
        .expect("session");
        (tmp, TodoStore::new(db), sid.to_string())
    }

    #[tokio::test]
    async fn replace_and_list() {
        let (_tmp, store, sid) = setup().await;
        let items = store
            .replace(
                &sid,
                vec![
                    TodoUpdate {
                        id: Some("todo_aaaaaaaaaaaaaaaa".into()),
                        content: Some("first".into()),
                        status: Some(TodoStatus::Pending),
                    },
                    TodoUpdate {
                        id: Some("todo_bbbbbbbbbbbbbbbb".into()),
                        content: Some("second".into()),
                        status: Some(TodoStatus::InProgress),
                    },
                ],
            )
            .await
            .expect("replace");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].content, "first");
        assert_eq!(items[1].status, TodoStatus::InProgress);
    }

    #[tokio::test]
    async fn merge_updates_status_only() {
        let (_tmp, store, sid) = setup().await;
        store
            .replace(
                &sid,
                vec![TodoUpdate {
                    id: Some("todo_cccccccccccccccc".into()),
                    content: Some("work".into()),
                    status: Some(TodoStatus::Pending),
                }],
            )
            .await
            .expect("replace");
        let items = store
            .merge(
                &sid,
                vec![TodoUpdate {
                    id: Some("todo_cccccccccccccccc".into()),
                    content: None,
                    status: Some(TodoStatus::Completed),
                }],
            )
            .await
            .expect("merge");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].content, "work");
        assert_eq!(items[0].status, TodoStatus::Completed);
        assert!(items[0].completed_at.is_some());
    }

    #[tokio::test]
    async fn reject_duplicate_ids() {
        let (_tmp, store, sid) = setup().await;
        let err = store
            .replace(
                &sid,
                vec![
                    TodoUpdate {
                        id: Some("todo_dddddddddddddddd".into()),
                        content: Some("a".into()),
                        status: None,
                    },
                    TodoUpdate {
                        id: Some("todo_dddddddddddddddd".into()),
                        content: Some("b".into()),
                        status: None,
                    },
                ],
            )
            .await
            .expect_err("dup");
        assert!(err.to_string().contains("duplicate"));
    }
}
