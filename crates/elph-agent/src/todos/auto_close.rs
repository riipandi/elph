//! Post-turn auto-close for stale todos.
//!
//! Some models finish their work but never call `todo_write` to mark items
//! completed — either they forget the status update entirely, or their
//! `completed` write is rejected because the work tracker cannot prove the
//! work per item. This module is the safety net: after a successful turn the
//! harness closes the items the turn actually finished.

use anyhow::Result;

use super::store::{TodoStore, TodoUpdate};
use super::tracker::WorkTracker;
use super::types::{TodoItem, TodoStatus};

/// Underlying reason attached to auto-closed todos (audit trail on tool
/// results; the table does not persist the reason column).
pub const AUTO_CLOSE_REASON: &str =
    "auto-closed at turn end: turn finished successfully after real work and the model never marked it completed";

/// Close open todos that a successful turn provably finished.
///
/// An item is considered done when the caller has already confirmed the final
/// assistant message carries a completion signal, and either:
/// - [`WorkTracker::has_work_since_snapshot`] proves work happened since the
///   item entered the plan (snapshots are taken when an item is created or
///   marked `in_progress`), or
/// - `turn_did_mutating_work` reports the turn performed at least one mutating
///   tool call (fallback for items the planner never touched in this process,
///   e.g. resumed sessions).
///
/// Returns the fresh list; unchanged when nothing was closed.
pub async fn auto_close_done_todos(
    store: &TodoStore,
    session_id: &str,
    work_tracker: &WorkTracker,
    turn_did_mutating_work: bool,
) -> Result<Vec<TodoItem>> {
    let items = store.list(session_id).await?;
    let open: Vec<&TodoItem> = items.iter().filter(|t| t.status.is_open()).collect();
    if open.is_empty() {
        return Ok(items);
    }

    let mut updates = Vec::new();
    for item in open {
        let done = work_tracker.has_work_since_snapshot(&item.id) || turn_did_mutating_work;
        if done {
            updates.push(TodoUpdate {
                id: Some(item.id.clone()),
                content: None,
                status: Some(TodoStatus::Completed),
                reason: Some(AUTO_CLOSE_REASON.into()),
            });
        }
    }
    if updates.is_empty() {
        Ok(items)
    } else {
        store.merge(session_id, updates).await
    }
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
        let sid = "sess_auto_close";
        c.execute(
            "INSERT INTO sessions (id, created_at, updated_at, cwd) VALUES (?, ?, ?, ?)",
            turso::params![sid, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "/tmp"],
        )
        .await
        .expect("session");
        (tmp, TodoStore::new(db), sid.to_string())
    }

    fn update(id: &str, status: TodoStatus) -> TodoUpdate {
        TodoUpdate {
            id: Some(id.into()),
            content: Some(id.into()),
            status: Some(status),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn no_open_items_is_noop() {
        let (_tmp, store, sid) = setup().await;
        let tracker = WorkTracker::new();
        let items = store
            .merge(
                &sid,
                vec![
                    update("todo_aaaaaaaaaaaaaaaa", TodoStatus::Completed),
                    update("todo_bbbbbbbbbbbbbbbb", TodoStatus::Cancelled),
                ],
            )
            .await
            .expect("seed");
        assert_eq!(items.len(), 2);
        let out = auto_close_done_todos(&store, &sid, &tracker, true).await.expect("noop");
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|t| !t.status.is_open()));
    }

    #[tokio::test]
    async fn in_progress_item_with_work_since_snapshot_closes() {
        let (_tmp, store, sid) = setup().await;
        let tracker = WorkTracker::new();
        let items = store
            .merge(&sid, vec![update("todo_aaaaaaaaaaaaaaaa", TodoStatus::InProgress)])
            .await
            .expect("seed");
        let id = items[0].id.clone();
        // Simulate the tool path: snapshot when marked in_progress, then work.
        tracker.snapshot_in_progress(&id);
        tracker.record_work();

        let out = auto_close_done_todos(&store, &sid, &tracker, false)
            .await
            .expect("close");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].status, TodoStatus::Completed);
        assert!(out[0].completed_at.is_some());
    }

    #[tokio::test]
    async fn pending_without_snapshot_closes_on_turn_work_only() {
        let (_tmp, store, sid) = setup().await;
        let tracker = WorkTracker::new();
        store
            .merge(&sid, vec![update("todo_aaaaaaaaaaaaaaaa", TodoStatus::Pending)])
            .await
            .expect("seed");

        // Turn did mutating work -> fallback closes it.
        let out = auto_close_done_todos(&store, &sid, &tracker, true)
            .await
            .expect("close");
        assert_eq!(out[0].status, TodoStatus::Completed);

        // Refresh list: pending again; turn did no work -> stays open.
        store
            .merge(&sid, vec![update("todo_aaaaaaaaaaaaaaaa", TodoStatus::Pending)])
            .await
            .expect("reset");
        let out = auto_close_done_todos(&store, &sid, &tracker, false)
            .await
            .expect("keep");
        assert_eq!(out[0].status, TodoStatus::Pending);
    }

    #[tokio::test]
    async fn mixes_only_close_proven_or_turn_worked() {
        let (_tmp, store, sid) = setup().await;
        let tracker = WorkTracker::new();
        let items = store
            .merge(
                &sid,
                vec![
                    update("todo_aaaaaaaaaaaaaaaa", TodoStatus::InProgress),
                    update("todo_bbbbbbbbbbbbbbbb", TodoStatus::Pending),
                    update("todo_cccccccccccccccc", TodoStatus::Completed),
                ],
            )
            .await
            .expect("seed");

        // Only item A is provable per-item (snapshot + work). Turn did work too,
        // so B closes via the fallback. C was already closed.
        tracker.snapshot_in_progress(&items[0].id);
        tracker.record_work();
        let out = auto_close_done_todos(&store, &sid, &tracker, true)
            .await
            .expect("close");
        let by_id = |id: &str| out.iter().find(|t| t.id == id).expect("present").status;
        assert_eq!(by_id(&items[0].id), TodoStatus::Completed);
        assert_eq!(by_id(&items[1].id), TodoStatus::Completed);
        assert_eq!(by_id(&items[2].id), TodoStatus::Completed);
    }

    #[tokio::test]
    async fn suggested_id_not_in_store_stays_open_without_work() {
        let (_tmp, store, sid) = setup().await;
        let tracker = WorkTracker::new();
        store
            .merge(&sid, vec![update("todo_aaaaaaaaaaaaaaaa", TodoStatus::Pending)])
            .await
            .expect("seed");
        // No snapshot for a different item; turn did no work -> neither closes.
        tracker.snapshot_in_progress("todo_dddddddddddddddd");
        tracker.record_work();
        let out = auto_close_done_todos(&store, &sid, &tracker, false)
            .await
            .expect("keep");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].status, TodoStatus::Pending);
    }
}
