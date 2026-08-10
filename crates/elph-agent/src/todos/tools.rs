//! Todo management tools for the agent harness.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, bail};
use serde_json::Value;
use serde_json::json;

use crate::todos::store::{TodoStore, TodoUpdate};
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
/// For each update that sets status to `completed`, verify the work tracker
/// has recorded work since the item was marked `in_progress`. For each update
/// that sets status to `in_progress`, snapshot the current work counter so we
/// can verify completion later.
///
/// Rejects the whole write if any `completed` item lacks proof of work.
fn enforce_work_done(
    _store: &TodoStore,
    _session_id: &str,
    updates: &[TodoUpdate],
    tracker: &WorkTracker,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    let tracker = tracker.clone();
    let updates = updates.to_vec();
    Box::pin(async move {
        // First, handle in_progress snapshots (no validation needed).
        for update in &updates {
            if update.status == Some(TodoStatus::InProgress) {
                if let Some(id) = &update.id {
                    tracker.snapshot_in_progress(id);
                }
            }
        }

        // Then, validate completed items have done real work.
        for update in &updates {
            if update.status == Some(TodoStatus::Completed) {
                let id = update.id.as_deref().unwrap_or("");
                if id.is_empty() {
                    continue;
                }
                // Allow bypass with a reason (analysis tasks, MCP work, etc.).
                let has_reason = update.reason.as_deref().map(str::trim).is_some_and(|r| !r.is_empty());
                if has_reason {
                    log::info!(
                        "todo '{id}' marked completed with reason: {}",
                        update.reason.as_deref().unwrap_or("")
                    );
                    continue;
                }
                if !tracker.has_work_since_snapshot(id) {
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
