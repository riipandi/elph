use std::sync::Arc;

use elph_agent::AgentToolResult;
use elph_agent::ensure_database;
use elph_agent::goals::create_goal_tools;
use elph_agent::goals::{GoalStatus, GoalStore};
use elph_agent::session::SESSION_TREE_MIGRATIONS;
use serde_json::json;

fn tool_text(result: AgentToolResult) -> String {
    result
        .content
        .into_iter()
        .filter_map(|block| match block {
            elph_agent::ToolResultContent::Text(text) => Some(text.text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Canonical schema + parent `sessions` row (goals FK requires it).
async fn setup_store(session_id: &str) -> (tempfile::TempDir, GoalStore) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("store.db");
    ensure_database(&db_path, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("migrate");
    let db = elph_agent::datastore::open_local(&db_path).await.expect("open");
    let conn = elph_agent::datastore::connect(&db).await.expect("connect");
    conn.execute(
        "INSERT INTO sessions (id, created_at, updated_at, cwd) VALUES (?, ?, ?, ?)",
        turso::params![session_id, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "/tmp"],
    )
    .await
    .expect("session");
    (tmp, GoalStore::new(&db_path))
}

#[tokio::test]
async fn goal_store_lifecycle() {
    let session_id = "sess_test";
    let (_tmp, store) = setup_store(session_id).await;

    let goal = store
        .create_goal(session_id, "Ship feature X", Some("tests pass"), 1000, 5, 60_000)
        .await
        .expect("create goal");
    assert_eq!(goal.objective, "Ship feature X");
    assert_eq!(goal.status, GoalStatus::Active);
    assert!(!goal.id.is_empty());
    assert!(goal.id.starts_with("goal_"));

    let active = store.get_active_goal(session_id).await.expect("get active");
    assert!(active.is_some());

    let err = store
        .create_goal(session_id, "Another goal", None, 0, 0, 0)
        .await
        .expect_err("duplicate active goal");
    assert!(err.to_string().contains("unfinished goal"));

    let paused = store.set_status(session_id, GoalStatus::Paused).await.expect("pause");
    assert_eq!(paused.status, GoalStatus::Paused);

    let resumed = store.resume_goal(session_id).await.expect("resume");
    assert_eq!(resumed.status, GoalStatus::Active);

    let updated = store
        .update_goal_status(session_id, GoalStatus::Complete)
        .await
        .expect("complete");
    assert_eq!(updated.status, GoalStatus::Complete);
    assert!(updated.completed_at.is_some());
}

#[tokio::test]
async fn goal_accounting_sets_budget_limited() {
    let session_id = "sess_budget";
    let (_tmp, store) = setup_store(session_id).await;
    store
        .create_goal(session_id, "Small task", None, 10, 0, 0)
        .await
        .expect("create");

    let goal = store
        .record_usage(session_id, 12, 1, 0)
        .await
        .expect("record")
        .expect("goal");
    assert_eq!(goal.status, GoalStatus::BudgetLimited);
    assert_eq!(goal.tokens_used, 12);
}

#[tokio::test]
async fn goal_tools_round_trip() {
    let session_id = "sess_tools";
    let (_tmp, store) = setup_store(session_id).await;
    let store = Arc::new(store);
    let tools = create_goal_tools(store, session_id.to_string());

    let create = tools.iter().find(|t| t.name() == "create_goal").expect("create_goal");
    let ctx = elph_agent::ToolContext::new(std::sync::Arc::new(elph_agent::LocalExecutionEnv::new(".")));
    let create_result = (create.execute)(
        "tc1".into(),
        json!({
            "objective": "Refactor module",
            "token_budget": 500
        }),
        None,
        None,
        ctx.clone(),
    )
    .await
    .expect("create tool");
    let create_text = tool_text(create_result);
    assert!(create_text.contains("Refactor module"));

    let update = tools.iter().find(|t| t.name() == "update_goal").expect("update_goal");
    let update_result = (update.execute)("tc4".into(), json!({ "status": "blocked" }), None, None, ctx.clone())
        .await
        .expect("update goal");
    assert!(tool_text(update_result).contains("\"status\":\"blocked\""));
}
