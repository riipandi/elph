//! Todo management tools for the agent harness.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use serde_json::json;

use crate::todos::store::{TodoStore, TodoUpdate};
use crate::todos::types::TodoStatus;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_todo_tools(store: Arc<TodoStore>, session_id: String) -> Vec<AgentTool> {
    vec![
        todo_write_tool(store.clone(), session_id.clone()),
        todo_read_tool(store, session_id),
    ]
}

fn todo_write_tool(store: Arc<TodoStore>, session_id: String) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "todo_write".into(),
            constrained_sampling: None,
            description: "Create or update the session todo list for multi-step work. \
                 Use merge=true (default) to upsert by id; merge=false replaces the whole list. \
                 Keep at most one item in_progress. Prefer for tasks with 3+ steps; skip trivial one-offs."
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
                                    "description": "Stable id (optional on create; required to update)"
                                },
                                "content": {
                                    "type": "string",
                                    "description": "Actionable title; optional on merge when only updating status"
                                },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed", "cancelled"],
                                    "description": "Task status"
                                }
                            }
                        }
                    }
                },
                "required": ["todos"]
            }),
        },
        "Update todos",
        move |_, args| todo_write_exec(store.clone(), session_id.clone(), args),
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
    args: Value,
) -> Pin<Box<dyn Future<Output = Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let merge = args.get("merge").and_then(|v| v.as_bool()).unwrap_or(true);
        let updates = parse_todo_updates(args.get("todos"))?;
        let items = if merge {
            if updates.is_empty() {
                store.list(&session_id).await?
            } else {
                store.merge(&session_id, updates).await?
            }
        } else {
            store.replace(&session_id, updates).await?
        };
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
        out.push(TodoUpdate { id, content, status });
    }
    Ok(out)
}
