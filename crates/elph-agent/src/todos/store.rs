//! Turso-backed session todo persistence.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use turso::Connection;

use crate::datastore::{connect, with_conn};
use crate::messages::now_iso_timestamp;
use crate::session::id::create_todo_id_checked;

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
        let items = normalize_updates(session_id, items)?;
        validate_updates(&items)?;
        let now = now_iso_timestamp();
        self.with_conn(|conn| async move {
            conn.execute("BEGIN IMMEDIATE", ()).await?;
            let outcome = async {
                conn.execute("DELETE FROM session_todos WHERE session_id = ?", turso::params![session_id])
                    .await?;
                // Track ids minted/inserted in this batch so sequential inserts never collide.
                let mut reserved = HashSet::new();
                for (position, update) in items.into_iter().enumerate() {
                    insert_todo(&conn, session_id, &update, position as i64, &now, &mut reserved).await?;
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
        let updates = normalize_updates(session_id, updates)?;
        validate_updates(&updates)?;
        let now = now_iso_timestamp();
        let existing = self.list(session_id).await?;
        let mut by_id: std::collections::HashMap<String, TodoItem> =
            existing.into_iter().map(|t| (t.id.clone(), t)).collect();

        for update in updates {
            // Id already resolved by normalize_updates (minted or session-scoped).
            let id = update
                .id
                .clone()
                .unwrap_or_else(|| create_todo_id_checked(&by_id.keys().cloned().collect()));
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
                ..Default::default()
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
#[derive(Debug, Clone, Default)]
pub struct TodoUpdate {
    pub id: Option<String>,
    pub content: Option<String>,
    pub status: Option<TodoStatus>,
    /// Optional reason for status change. When set on a `completed` transition,
    /// bypasses the work-done check (e.g. analysis tasks, MCP-driven work).
    /// Provides an audit trail for completions without local tool calls.
    pub reason: Option<String>,
}

/// Resolve agent-facing ids into globally unique PKs before write.
///
/// `session_todos.id` is a **global** PRIMARY KEY. Models often pass short labels
/// (`"1"`, `"2"`, `"todo_1"`). Those collide across sessions. We map unsafe ids to
/// a deterministic session-scoped form so merge-by-id stays stable within a session
/// and never hits another session's row.
fn normalize_updates(session_id: &str, updates: Vec<TodoUpdate>) -> Result<Vec<TodoUpdate>> {
    let mut reserved = HashSet::new();
    let mut out = Vec::with_capacity(updates.len());
    for mut u in updates {
        let resolved = resolve_todo_id(session_id, u.id.as_deref(), &reserved);
        if !reserved.insert(resolved.clone()) {
            bail!("duplicate todo id in one call: {resolved}");
        }
        u.id = Some(resolved);
        out.push(u);
    }
    Ok(out)
}

/// Public for unit tests — map optional agent id → store PK.
pub fn resolve_todo_id(session_id: &str, agent_id: Option<&str>, reserved: &HashSet<String>) -> String {
    match agent_id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) if is_global_safe_todo_id(raw) => raw.to_string(),
        Some(raw) => session_scoped_todo_id(session_id, raw),
        None => create_todo_id_checked(reserved),
    }
}

/// Host-minted `todo_<16>` kalids and previously scoped `td_*` keys are global-safe.
fn is_global_safe_todo_id(id: &str) -> bool {
    if let Some(body) = id.strip_prefix("todo_") {
        // Standard mint: todo_ + 16-char kalid body.
        return body.len() == 16 && body.chars().all(|c| c.is_ascii_alphanumeric());
    }
    // Session-scoped ids from a prior write of this host.
    id.starts_with("td_")
}

/// Stable session-local id for short agent labels (`"1"` → `td_<sess12>_<slug>`).
fn session_scoped_todo_id(session_id: &str, agent_id: &str) -> String {
    let sid: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    let slug: String = agent_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(48)
        .collect();
    let slug = if slug.is_empty() { "x".to_string() } else { slug };
    format!("td_{sid}_{slug}")
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

async fn insert_todo(
    conn: &Connection,
    session_id: &str,
    update: &TodoUpdate,
    position: i64,
    now: &str,
    reserved: &mut HashSet<String>,
) -> Result<()> {
    let id = match update.id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => id.to_string(),
        None => create_todo_id_checked(reserved),
    };
    reserved.insert(id.clone());
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
    .await
    .with_context(|| format!("insert session_todos id={id}"))?;
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

    async fn insert_session(store: &TodoStore, sid: &str) {
        let db = store.db_path();
        let conn = crate::datastore::open_local(db).await.expect("open");
        let c = crate::datastore::connect(&conn).await.expect("connect");
        c.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, updated_at, cwd) VALUES (?, ?, ?, ?)",
            turso::params![sid, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "/tmp"],
        )
        .await
        .expect("session");
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
                        ..Default::default()
                    },
                    TodoUpdate {
                        id: Some("todo_bbbbbbbbbbbbbbbb".into()),
                        content: Some("second".into()),
                        status: Some(TodoStatus::InProgress),
                        ..Default::default()
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
                    ..Default::default()
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
                    ..Default::default()
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
                        ..Default::default()
                    },
                    TodoUpdate {
                        id: Some("todo_dddddddddddddddd".into()),
                        content: Some("b".into()),
                        status: None,
                        ..Default::default()
                    },
                ],
            )
            .await
            .expect_err("dup");
        assert!(err.to_string().contains("duplicate"));
    }

    #[tokio::test]
    async fn short_agent_ids_do_not_collide_across_sessions() {
        let (_tmp, store, sid_a) = setup().await;
        let sid_b = "sess_other_sess";
        insert_session(&store, sid_b).await;

        // Both sessions write todos with the same short agent ids ("1", "2").
        let a = store
            .merge(
                &sid_a,
                vec![
                    TodoUpdate {
                        id: Some("1".into()),
                        content: Some("A1".into()),
                        status: Some(TodoStatus::InProgress),
                        ..Default::default()
                    },
                    TodoUpdate {
                        id: Some("2".into()),
                        content: Some("A2".into()),
                        status: Some(TodoStatus::Pending),
                        ..Default::default()
                    },
                ],
            )
            .await
            .expect("session A");
        let b = store
            .merge(
                &sid_b,
                vec![
                    TodoUpdate {
                        id: Some("1".into()),
                        content: Some("B1".into()),
                        status: Some(TodoStatus::InProgress),
                        ..Default::default()
                    },
                    TodoUpdate {
                        id: Some("2".into()),
                        content: Some("B2".into()),
                        status: Some(TodoStatus::Pending),
                        ..Default::default()
                    },
                ],
            )
            .await
            .expect("session B");

        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        assert_ne!(a[0].id, b[0].id);
        assert!(a[0].id.starts_with("td_"));
        assert!(b[0].id.starts_with("td_"));

        // Re-merge by short id still hits the same row in this session.
        let a2 = store
            .merge(
                &sid_a,
                vec![TodoUpdate {
                    id: Some("1".into()),
                    content: None,
                    status: Some(TodoStatus::Completed),
                    ..Default::default()
                }],
            )
            .await
            .expect("remerge A");
        let one = a2.iter().find(|t| t.content == "A1").expect("A1");
        assert_eq!(one.status, TodoStatus::Completed);
        assert_eq!(a2.len(), 2);
    }

    #[test]
    fn resolve_keeps_minted_kalid_and_scopes_short() {
        let empty = HashSet::new();
        assert_eq!(
            resolve_todo_id("sess_abc", Some("todo_abcdefghijklmnop"), &empty),
            "todo_abcdefghijklmnop"
        );
        let scoped = resolve_todo_id("sess_abc", Some("1"), &empty);
        assert!(scoped.starts_with("td_"), "{scoped}");
        assert!(scoped.contains('1'), "{scoped}");
        // Same inputs → same id (stable merge key).
        assert_eq!(scoped, resolve_todo_id("sess_abc", Some("1"), &empty));
    }

    #[tokio::test]
    async fn mint_without_id_is_unique_in_batch() {
        let (_tmp, store, sid) = setup().await;
        let items = store
            .replace(
                &sid,
                vec![
                    TodoUpdate {
                        id: None,
                        content: Some("a".into()),
                        status: Some(TodoStatus::Pending),
                        ..Default::default()
                    },
                    TodoUpdate {
                        id: None,
                        content: Some("b".into()),
                        status: Some(TodoStatus::Pending),
                        ..Default::default()
                    },
                    TodoUpdate {
                        id: None,
                        content: Some("c".into()),
                        status: Some(TodoStatus::Pending),
                        ..Default::default()
                    },
                ],
            )
            .await
            .expect("mint batch");
        assert_eq!(items.len(), 3);
        let ids: HashSet<_> = items.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 3);
    }
}
