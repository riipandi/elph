//! Agent tool for reading session summaries.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use serde_json::json;

use crate::session_summary::store::SessionSummaryStore;
use crate::session_summary::types::SessionSummary;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

/// Create the `get_session_summary` agent tool.
///
/// The tool takes a `session_id` argument and returns the stored compaction
/// summary for that session, or a "not found" message when no summary exists.
pub fn create_session_summary_tool(store: Arc<SessionSummaryStore>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "get_session_summary".into(),
            constrained_sampling: None,
            description: "Retrieve the stored compaction summary for a session. \
                Use to recall context from a different session (e.g. past work, decisions, \
                or file operations) without replaying full history. Returns the latest \
                summary text plus token/compaction metadata."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "Session ID to look up the summary for."
                    }
                },
                "required": ["session_id"]
            }),
        },
        "Get session summary",
        move |_, args| get_session_summary_exec(store.clone(), args),
    )
}

fn get_session_summary_exec(
    store: Arc<SessionSummaryStore>,
    args: Value,
) -> Pin<Box<dyn Future<Output = Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let session_id = args
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: session_id"))?;

        match store.get(session_id).await? {
            Some(summary) => Ok(AgentToolResult::text(format_summary(&summary))),
            None => Ok(AgentToolResult::text(format!(
                "No session summary found for session '{session_id}'."
            ))),
        }
    })
}

fn format_summary(summary: &SessionSummary) -> String {
    let details = summary.details.as_deref().unwrap_or("none");
    format!(
        "Session: {session_id}\n\
                 Compactions: {compaction_count}\n\
                 Tokens before last: {tokens_before}\n\
                 First kept entry: {first_kept}\n\
                 Updated: {updated_at}\n\
                 Details: {details}\n\
                 \n\
                 --- Summary ---\n\
                 {summary_text}",
        session_id = summary.session_id,
        compaction_count = summary.compaction_count,
        tokens_before = summary.tokens_before,
        first_kept = summary.first_kept_entry_id.as_deref().unwrap_or("n/a"),
        updated_at = summary.updated_at,
        details = details,
        summary_text = summary.summary,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_store() -> (tempfile::TempDir, Arc<SessionSummaryStore>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("test.db");
        let store = Arc::new(SessionSummaryStore::new(db_path.clone()));

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

    fn exec_tool(
        tool: &AgentTool,
        args: Value,
    ) -> std::pin::Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
        let ctx = crate::types::ToolContext::new(std::sync::Arc::new(
            crate::runtime::local_env::LocalExecutionEnv::new(std::path::Path::new(".")),
        ));
        (tool.execute)("call_test".into(), args, None, None, ctx)
    }

    fn result_text(result: AgentToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[tokio::test]
    async fn tool_returns_summary_when_found() {
        let (_tmp, store) = setup_store().await;
        store
            .upsert("sess_tool", "test summary", 800, 1, Some("e1"), None)
            .await
            .expect("upsert");

        let tool = create_session_summary_tool(store);
        let result = exec_tool(&tool, json!({"session_id": "sess_tool"}))
            .await
            .expect("exec");
        let text = result_text(result);
        assert!(text.contains("test summary"));
        assert!(text.contains("sess_tool"));
    }

    #[tokio::test]
    async fn tool_returns_not_found_when_missing() {
        let (_tmp, store) = setup_store().await;
        let tool = create_session_summary_tool(store);
        let result = exec_tool(&tool, json!({"session_id": "sess_missing"}))
            .await
            .expect("exec");
        let text = result_text(result);
        assert!(text.contains("No session summary"));
    }

    #[tokio::test]
    async fn tool_errors_without_session_id() {
        let (_tmp, store) = setup_store().await;
        let tool = create_session_summary_tool(store);
        let result = exec_tool(&tool, json!({})).await;
        assert!(result.is_err());
    }
}
