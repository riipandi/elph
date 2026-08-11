//! Todo management tools for the agent harness.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, bail};
use serde_json::Value;
use serde_json::json;

use crate::todos::store::{TodoStore, TodoUpdate, resolve_todo_id};
use crate::todos::tracker::WorkTracker;
use crate::todos::types::{TodoItem, TodoStatus};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

/// Optional hook after todo list changes (e.g. UI event emission).
pub type TodoHook = Arc<dyn Fn(Vec<TodoItem>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Work-tracker handle for enforcing honest progress.
///
/// When set, `todo_write` verifies that actual work (mutating tool calls) was
/// done between marking an item `in_progress` and marking it `completed`.
/// Prevents the agent from reporting false progress.
pub type WorkTrackerHandle = std::sync::Arc<crate::todos::tracker::WorkTracker>;

pub fn create_todo_tools(store: Arc<TodoStore>, session_id: String) -> Vec<AgentTool> {
    create_todo_tools_with_hook(store, session_id, None, None)
}

/// Same as [`create_todo_tools`], with optional hooks for UI updates and work tracking.
pub fn create_todo_tools_with_hook(
    store: Arc<TodoStore>,
    session_id: String,
    on_update: Option<TodoHook>,
    work_tracker: Option<WorkTrackerHandle>,
) -> Vec<AgentTool> {
    vec![
        todo_write_tool(store.clone(), session_id.clone(), on_update, work_tracker),
        todo_read_tool(store, session_id),
    ]
}

fn todo_write_tool(
    store: Arc<TodoStore>,
    session_id: String,
    on_update: Option<TodoHook>,
    work_tracker: Option<WorkTrackerHandle>,
) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "todo_write".into(),
            constrained_sampling: None,
            description: "Create or update the session todo list for multi-step work. \
                 Use merge=true (default) to upsert by id; merge=false replaces the whole list. \
                 Keep at most one item in_progress. Prefer for tasks with 3+ steps; skip trivial one-offs. \
                 Short ids like \"1\"/\"2\" are fine — the host scopes them per session. Prefer reusing ids from the tool result on later updates. \
                 Status `completed` requires actual work since `in_progress` (a mutating tool call). \
                 For tasks that don't involve local tool calls (analysis, review, MCP-driven work), \
                 pass a `reason` explaining completion to bypass the work check."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "merge": {
                        "type": "boolean",
                        "description": "When true (default), upsert by id. When false, replace the entire list.",
                        "default": true
                    },
                    "todos": {
                        "type": "array",
                        "description": "Todo items to write. Empty array with merge=false clears the list.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Stable id (optional on create; required to update). Short labels are session-scoped by the host; use ids returned by the tool for later merges."
                                },
                                "content": {
                                    "type": "string",
                                    "description": "Actionable title; optional on merge when only updating status"
                                },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed", "cancelled"],
                                    "description": "Task status"
                                },
                                "reason": {
                                    "type": "string",
                                    "description": "Optional reason for the status change. Required when marking `completed` without local mutating tool calls (e.g. analysis, review, MCP-driven work). Provides audit trail for the completion."
                                }
                            }
                        }
                    }
                },
                "required": ["todos"]
            }),
        },
        "Update todos",
        move |_, args| {
            todo_write_exec(
                store.clone(),
                session_id.clone(),
                on_update.clone(),
                work_tracker.clone(),
                args,
            )
        },
    )
}

fn todo_read_tool(store: Arc<TodoStore>, session_id: String) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "todo_read".into(),
            constrained_sampling: None,
            description: "Read the current session todo list (status and content).".into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        "Read todos",
        move |_, _| todo_read_exec(store.clone(), session_id.clone()),
    )
}

fn todo_write_exec(
    store: Arc<TodoStore>,
    session_id: String,
    on_update: Option<TodoHook>,
    work_tracker: Option<WorkTrackerHandle>,
    args: Value,
) -> Pin<Box<dyn Future<Output = Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let merge = args.get("merge").and_then(|v| v.as_bool()).unwrap_or(true);
        let updates = parse_todo_updates(args.get("todos"))?;

        // Enforce honest progress: before allowing `completed`, verify that
        // actual work was done since the item was marked `in_progress`.
        if let Some(tracker) = work_tracker {
            enforce_work_done(&store, &session_id, &updates, &tracker).await?;
        }

        let items = if merge {
            if updates.is_empty() {
                store.list(&session_id).await?
            } else {
                store.merge(&session_id, updates).await?
            }
        } else {
            store.replace(&session_id, updates).await?
        };
        // Emit UI event if hook is provided
        if let Some(hook) = on_update {
            hook(items.clone()).await;
        }
        let body = serde_json::to_string_pretty(&items)?;
        Ok(AgentToolResult::text(body))
    })
}

fn todo_read_exec(
    store: Arc<TodoStore>,
    session_id: String,
) -> Pin<Box<dyn Future<Output = Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let items = store.list(&session_id).await?;
        let body = serde_json::to_string_pretty(&items)?;
        Ok(AgentToolResult::text(body))
    })
}

fn parse_todo_updates(value: Option<&Value>) -> Result<Vec<TodoUpdate>> {
    let Some(arr) = value.and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let content = item.get("content").and_then(|v| v.as_str()).map(|s| s.to_string());
        let status = item.get("status").and_then(|v| v.as_str()).and_then(TodoStatus::parse);
        let reason = item
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        out.push(TodoUpdate {
            id,
            content,
            status,
            reason,
        });
    }
    Ok(out)
}

/// Enforce that `completed` transitions are backed by actual work.
///
/// Snapshots the work counter when an item is marked `in_progress` — and when
/// a brand-new item enters the plan — so completion can prove work happened
/// after the item was created. Models routinely skip the `in_progress` step
/// and mark done items `completed` at the very end; without the creation
/// baseline those writes would be rejected even though real work occurred.
///
/// Rejects the whole write if any `completed` item lacks proof of work.
fn enforce_work_done(
    store: &TodoStore,
    session_id: &str,
    updates: &[TodoUpdate],
    tracker: &WorkTracker,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    let store = store.clone();
    let session_id = session_id.to_string();
    let tracker = tracker.clone();
    let updates = updates.to_vec();
    Box::pin(async move {
        // Snapshots are keyed by the resolved store PK (agent short ids like
        // "1" map to `td_<session>_<slug>`), so the post-turn auto-close hook —
        // which reads real `TodoItem.id`s — sees the same keys.
        let existing = store.list(&session_id).await?;
        let existing_ids: HashSet<String> = existing.into_iter().map(|t| t.id).collect();

        // Baseline the counter for items that do not exist yet. Creation and
        // in_progress both refresh the baseline to "now".
        for update in &updates {
            if update.status != Some(TodoStatus::InProgress)
                && let Some(id) = update.id.as_deref()
            {
                let resolved = resolve_todo_id(&session_id, Some(id), &HashSet::new());
                if !existing_ids.contains(&resolved) {
                    tracker.snapshot_in_progress(&resolved);
                }
            }
        }

        // Then, handle in_progress snapshots (no validation needed).
        for update in &updates {
            if update.status == Some(TodoStatus::InProgress)
                && let Some(id) = update.id.as_deref()
            {
                tracker.snapshot_in_progress(&resolve_todo_id(&session_id, Some(id), &HashSet::new()));
            }
        }

        // Finally, validate completed items have done real work.
        for update in &updates {
            if update.status == Some(TodoStatus::Completed) {
                let Some(id) = update.id.as_deref() else {
                    continue;
                };
                let resolved = resolve_todo_id(&session_id, Some(id), &HashSet::new());
                // Allow bypass with a reason (analysis tasks, MCP work, etc.).
                let has_reason = update.reason.as_deref().map(str::trim).is_some_and(|r| !r.is_empty());
                if has_reason {
                    log::info!(
                        "todo '{id}' marked completed with reason: {}",
                        update.reason.as_deref().unwrap_or("")
                    );
                    continue;
                }
                if !tracker.has_work_since_snapshot(&resolved) {
                    bail!(
                        "Cannot mark todo '{id}' as completed: no actual work was recorded since it was marked in_progress. \
                         Do the work first (edit files, run commands, etc.), then update status. \
                         If this task did not require local tool calls (e.g. analysis, review, MCP-driven work), \
                         provide a `reason` explaining why it is done."
                    );
                }
            }
        }

        Ok(())
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
        let sid = "sess_enforce_ws";
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
            status: Some(status),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn completion_passes_when_created_then_worked_on() {
        let (_tmp, store, sid) = setup().await;
        let tracker = WorkTracker::new();

        // Creation write (item "1" does not exist yet): baseline snapshot taken.
        enforce_work_done(&store, &sid, &[update("1", TodoStatus::Pending)], &tracker)
            .await
            .expect("create accepted");
        store
            .merge(&sid, vec![update("1", TodoStatus::Pending)])
            .await
            .expect("persist");
        let items = store.list(&sid).await.expect("list");
        let pk = items[0].id.clone();
        assert!(pk.starts_with("td_"), "short id scoped to PK: {pk}");

        // Real work happens, then the model completes the item in a later write.
        tracker.record_work();
        enforce_work_done(&store, &sid, &[update("1", TodoStatus::Completed)], &tracker)
            .await
            .expect("completion accepted");
        store
            .merge(&sid, vec![update("1", TodoStatus::Completed)])
            .await
            .expect("persist");
        assert_eq!(store.list(&sid).await.expect("list")[0].status, TodoStatus::Completed);

        // The same snapshot proves work for the post-turn auto-close hook too.
        assert!(tracker.has_work_since_snapshot(&pk));
    }

    #[tokio::test]
    async fn completion_without_any_work_after_creation_is_rejected() {
        let (_tmp, store, sid) = setup().await;
        let tracker = WorkTracker::new();

        enforce_work_done(&store, &sid, &[update("1", TodoStatus::Pending)], &tracker)
            .await
            .expect("create accepted");
        // No work recorded -> completing must be rejected.
        let err = enforce_work_done(&store, &sid, &[update("1", TodoStatus::Completed)], &tracker)
            .await
            .expect_err("no work");
        assert!(err.to_string().contains("no actual work"), "{err}");
    }

    #[tokio::test]
    async fn create_and_complete_in_same_call_is_rejected() {
        let (_tmp, store, sid) = setup().await;
        let tracker = WorkTracker::new();
        // The baseline is taken inside the same call, so work-after-creation
        // cannot be proven — the model must do the work first, then complete.
        let err = enforce_work_done(&store, &sid, &[update("2", TodoStatus::Completed)], &tracker)
            .await
            .expect_err("no work");
        assert!(err.to_string().contains("no actual work"), "{err}");
    }

    #[tokio::test]
    async fn in_progress_snapshot_still_refreshes_baseline() {
        let (_tmp, store, sid) = setup().await;
        let tracker = WorkTracker::new();

        // Created earlier (baseline at 0), some pre-work happened.
        enforce_work_done(&store, &sid, &[update("1", TodoStatus::Pending)], &tracker)
            .await
            .expect("create");
        store
            .merge(&sid, vec![update("1", TodoStatus::Pending)])
            .await
            .expect("persist");
        tracker.record_work();

        // Mark in_progress now: baseline refreshes, so pre-existing work no
        // longer counts — real work must follow the in_progress transition.
        enforce_work_done(&store, &sid, &[update("1", TodoStatus::InProgress)], &tracker)
            .await
            .expect("in_progress");
        let err = enforce_work_done(&store, &sid, &[update("1", TodoStatus::Completed)], &tracker)
            .await
            .expect_err("no work since in_progress");
        assert!(err.to_string().contains("no actual work"), "{err}");

        tracker.record_work();
        enforce_work_done(&store, &sid, &[update("1", TodoStatus::Completed)], &tracker)
            .await
            .expect("work after in_progress");
    }
}
