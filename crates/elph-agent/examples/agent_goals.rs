//! Goal lifecycle — create, track status, update, and report budget usage.
//!
//! Demonstrates: `Goal`, `GoalStore`, `GoalStatus`, `GoalRuntime`, `GoalTurnStart`, `GoalTurnFinish`,
//! `create_goal_tools` tool definitions.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_goals --features builtin-tools
//! ```

use std::sync::Arc;

use elph_agent::goals::{GoalRuntime, GoalStatus, GoalStore, create_goal_tools};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let db_path = tmp.path().join("goals.db");
    let session_id = "session_demo_001";

    // ── 1. GoalStore: create, get latest, update ──
    println!("=== GoalStore ===");
    let store = Arc::new(GoalStore::new(&db_path));

    // Create a goal with budgets
    let goal = store
        .create_goal(
            session_id,
            "Refactor auth module",
            Some("All auth tests pass"),
            50000,
            20,
            600_000,
        )
        .await?;
    println!("  Created:  goal_id={}, objective={}", goal.goal_id, goal.objective);
    println!("  Status:   {:?}", goal.status);
    println!(
        "  Budgets:  {} tokens, {} turns, {}ms wall-clock",
        goal.token_budget, goal.turn_budget, goal.wall_clock_budget_ms
    );

    // Get latest goal
    let latest = store.get_latest_goal(session_id).await?.expect("goal exists");
    assert_eq!(latest.goal_id, goal.goal_id);
    println!("  Latest:   goal_id={}, status={:?}", latest.goal_id, latest.status);

    // Update status to Complete
    let updated = store.update_goal_status(session_id, GoalStatus::Complete).await?;
    println!(
        "  Updated:  status={:?}, completed_at={:?}",
        updated.status, updated.completed_at
    );

    // ── 2. GoalStatus variants ──
    println!("\n=== GoalStatus ===");
    for status in &[
        GoalStatus::Active,
        GoalStatus::Complete,
        GoalStatus::Blocked,
        GoalStatus::Paused,
        GoalStatus::BudgetLimited,
        GoalStatus::UsageLimited,
    ] {
        println!(
            "  {:15}  as_str={:15}  terminal={:5}  blocks_turns={:5}",
            format!("{status:?}"),
            format!("{:?}", status.as_str()),
            status.is_terminal(),
            status.blocks_turns(),
        );
    }

    // ── 3. GoalRuntime: turn lifecycle ──
    println!("\n=== GoalRuntime ===");

    // Create a new active goal for runtime demo
    store
        .create_goal(session_id, "Implement search feature", None, 100_000, 50, 1_200_000)
        .await?;

    let runtime = GoalRuntime::new(store.clone(), session_id);

    // Start a turn (should be Ok since goal is active)
    let turn_start = runtime
        .start_turn(elph_agent::collaboration::CollaborationMode::Default)
        .await;
    println!("  turn_start: {:?}", turn_start);

    // Simulate some work
    let fake_usage = elph_ai::Usage {
        input: 1500,
        output: 3200,
        total_tokens: 4700,
        ..Default::default()
    };

    // Finish the turn
    let turn_finish = runtime
        .finish_turn(elph_agent::collaboration::CollaborationMode::Default, Some(&fake_usage))
        .await;
    println!("  turn_finish: {:?}", turn_finish);

    // Check goal after
    let after = store.get_latest_goal(session_id).await?.expect("goal exists");
    println!(
        "  after turn: turns_used={}, tokens_used={}, wall_clock_ms={}",
        after.turns_used, after.tokens_used, after.wall_clock_ms,
    );

    // ── 4. create_goal_tools: inspect tool definitions ──
    println!("\n=== Goal Tools Definition ===");
    let goal_tools = create_goal_tools(store.clone(), session_id.to_string());
    for tool in &goal_tools {
        println!("  tool: {} — {}", tool.tool.name, tool.label);
    }

    // ── 5. Goal struct fields ──
    println!("\n=== Goal Struct ===");
    let g = store.get_latest_goal(session_id).await?.expect("goal exists");
    println!("  id:                   {}", g.id);
    println!("  goal_id:              {}", g.goal_id);
    println!("  session_id:           {}", g.session_id);
    println!("  objective:            {}", g.objective);
    println!("  completion_criterion: {:?}", g.completion_criterion);
    println!("  status:               {:?}", g.status);
    println!("  turns_used:           {}", g.turns_used);
    println!("  tokens_used:          {}", g.tokens_used);
    println!("  wall_clock_ms:        {}", g.wall_clock_ms);
    println!("  token_budget:         {}", g.token_budget);
    println!("  turn_budget:          {}", g.turn_budget);
    println!("  wall_clock_budget_ms: {}", g.wall_clock_budget_ms);
    println!("  created_at:           {}", g.created_at);
    println!("  completed_at:         {:?}", g.completed_at);

    println!("\nDone.");
    Ok(())
}
