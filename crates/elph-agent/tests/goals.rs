use std::sync::Arc;

use elph_agent::goals::{GoalStatus, GoalStore, create_goal_tools};
use elph_agent::{AgentToolResult, Migration, ensure_database};
use serde_json::json;

const GOALS_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 4,
        name: "create_goals_table",
        up: "CREATE TABLE IF NOT EXISTS goals (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            objective TEXT NOT NULL,
            completion_criterion TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            turns_used INTEGER NOT NULL DEFAULT 0,
            tokens_used INTEGER NOT NULL DEFAULT 0,
            wall_clock_ms INTEGER NOT NULL DEFAULT 0,
            wall_clock_budget_ms INTEGER NOT NULL DEFAULT 0,
            turn_budget INTEGER NOT NULL DEFAULT 0,
            token_budget INTEGER NOT NULL DEFAULT 0,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            completed_at DATETIME,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_goals_session_id ON goals(session_id);
        CREATE INDEX IF NOT EXISTS idx_goals_status ON goals(status);",
    },
    Migration {
        version: 6,
        name: "add_goal_id_column",
        up: "ALTER TABLE goals ADD COLUMN goal_id TEXT;
            CREATE INDEX IF NOT EXISTS idx_goals_goal_id ON goals(goal_id);",
    },
];

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

#[tokio::test]
async fn goal_store_lifecycle() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("metadata.db");
    ensure_database(&db_path, GOALS_MIGRATIONS).await.expect("migrate");

    let store = GoalStore::new(&db_path);
    let session_id = "sess_test";

    let goal = store
        .create_goal(session_id, "Ship feature X", Some("tests pass"), 1000, 5, 60_000)
        .await
        .expect("create goal");
    assert_eq!(goal.objective, "Ship feature X");
    assert_eq!(goal.status, GoalStatus::Active);
    assert!(!goal.goal_id.is_empty());

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
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("metadata.db");
    ensure_database(&db_path, GOALS_MIGRATIONS).await.expect("migrate");

    let store = GoalStore::new(&db_path);
    let session_id = "sess_budget";
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
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("metadata.db");
    ensure_database(&db_path, GOALS_MIGRATIONS).await.expect("migrate");

    let store = Arc::new(GoalStore::new(&db_path));
    let session_id = "sess_tools".to_string();
    let tools = create_goal_tools(store, session_id);

    let create = tools.iter().find(|t| t.name() == "create_goal").expect("create_goal");
    let create_result = (create.execute)(
        "tc1".into(),
        json!({
            "objective": "Refactor module",
            "token_budget": 500
        }),
        None,
        None,
    )
    .await
    .expect("create tool");
    let create_text = tool_text(create_result);
    assert!(create_text.contains("Refactor module"));

    let update = tools.iter().find(|t| t.name() == "update_goal").expect("update_goal");
    let update_result = (update.execute)("tc4".into(), json!({ "status": "blocked" }), None, None)
        .await
        .expect("update goal");
    assert!(tool_text(update_result).contains("\"status\":\"blocked\""));
}
