//! Goal management tools for the agent harness.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{Value, json};

use crate::goals::store::GoalStore;
use crate::goals::types::{Goal, GoalStatus};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_goal_tools(store: Arc<GoalStore>, session_id: String) -> Vec<AgentTool> {
    vec![
        create_goal_tool(store.clone(), session_id.clone()),
        get_goal_tool(store.clone(), session_id.clone()),
        update_goal_tool(store.clone(), session_id.clone()),
        set_goal_budget_tool(store, session_id),
    ]
}

fn create_goal_tool(store: Arc<GoalStore>, session_id: String) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "create_goal".into(),
            description: "Create a session goal with an objective and optional budgets.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "objective": {
                        "type": "string",
                        "description": "Concrete objective for this session"
                    },
                    "completion_criterion": {
                        "type": "string",
                        "description": "Optional definition of done"
                    },
                    "token_budget": {
                        "type": "integer",
                        "description": "Optional token budget (0 = unlimited)"
                    },
                    "turn_budget": {
                        "type": "integer",
                        "description": "Optional turn budget (0 = unlimited)"
                    },
                    "wall_clock_budget_ms": {
                        "type": "integer",
                        "description": "Optional wall-clock budget in milliseconds (0 = unlimited)"
                    }
                },
                "required": ["objective"]
            }),
        },
        "Create goal",
        move |_, args| create_goal_exec(store.clone(), session_id.clone(), args),
    )
}

fn get_goal_tool(store: Arc<GoalStore>, session_id: String) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "get_goal".into(),
            description: "Get the current session goal status and remaining budgets.".into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        "Get goal",
        move |_, _| get_goal_exec(store.clone(), session_id.clone()),
    )
}

fn update_goal_tool(store: Arc<GoalStore>, session_id: String) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "update_goal".into(),
            description: "Update the active goal status to complete or blocked.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["complete", "blocked"],
                        "description": "New goal status"
                    }
                },
                "required": ["status"]
            }),
        },
        "Update goal",
        move |_, args| update_goal_exec(store.clone(), session_id.clone(), args),
    )
}

fn set_goal_budget_tool(store: Arc<GoalStore>, session_id: String) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "set_goal_budget".into(),
            description: "Set token, turn, or wall-clock budget on the active goal.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "token_budget": { "type": "integer" },
                    "turn_budget": { "type": "integer" },
                    "wall_clock_budget_ms": { "type": "integer" }
                }
            }),
        },
        "Set goal budget",
        move |_, args| set_goal_budget_exec(store.clone(), session_id.clone(), args),
    )
}

fn create_goal_exec(
    store: Arc<GoalStore>,
    session_id: String,
    args: Value,
) -> Pin<Box<dyn Future<Output = Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let objective = args.get("objective").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let completion_criterion = args
            .get("completion_criterion")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let token_budget = args.get("token_budget").and_then(|v| v.as_i64()).unwrap_or(0);
        let turn_budget = args.get("turn_budget").and_then(|v| v.as_i64()).unwrap_or(0);
        let wall_clock_budget_ms = args.get("wall_clock_budget_ms").and_then(|v| v.as_i64()).unwrap_or(0);

        let goal = store
            .create_goal(
                &session_id,
                &objective,
                completion_criterion.as_deref(),
                token_budget,
                turn_budget,
                wall_clock_budget_ms,
            )
            .await?;
        Ok(AgentToolResult::text(goal_to_json(&goal)?.to_string()))
    })
}

fn get_goal_exec(
    store: Arc<GoalStore>,
    session_id: String,
) -> Pin<Box<dyn Future<Output = Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let goal = store.get_latest_goal(&session_id).await?;
        let payload = match goal {
            Some(goal) => goal_to_json(&goal)?,
            None => json!({ "goal": null, "message": "no goal for this session" }),
        };
        Ok(AgentToolResult::text(payload.to_string()))
    })
}

fn update_goal_exec(
    store: Arc<GoalStore>,
    session_id: String,
    args: Value,
) -> Pin<Box<dyn Future<Output = Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let status_str = args
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("status is required"))?;
        let status = match status_str {
            "complete" => GoalStatus::Complete,
            "blocked" => GoalStatus::Blocked,
            _ => anyhow::bail!("status must be complete or blocked"),
        };

        let goal = store.update_goal_status(&session_id, status).await?;
        Ok(AgentToolResult::text(goal_to_json(&goal)?.to_string()))
    })
}

fn set_goal_budget_exec(
    store: Arc<GoalStore>,
    session_id: String,
    args: Value,
) -> Pin<Box<dyn Future<Output = Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let goal = store
            .set_goal_budget(
                &session_id,
                args.get("token_budget").and_then(|v| v.as_i64()),
                args.get("turn_budget").and_then(|v| v.as_i64()),
                args.get("wall_clock_budget_ms").and_then(|v| v.as_i64()),
            )
            .await?;
        Ok(AgentToolResult::text(goal_to_json(&goal)?.to_string()))
    })
}

fn goal_to_json(goal: &Goal) -> Result<Value> {
    let remaining_tokens = remaining(goal.token_budget, goal.tokens_used);
    Ok(json!({
        "id": goal.id,
        "goal_id": goal.goal_id,
        "session_id": goal.session_id,
        "objective": goal.objective,
        "completion_criterion": goal.completion_criterion,
        "status": goal.status.as_str(),
        "turns_used": goal.turns_used,
        "tokens_used": goal.tokens_used,
        "wall_clock_ms": goal.wall_clock_ms,
        "wall_clock_budget_ms": goal.wall_clock_budget_ms,
        "turn_budget": goal.turn_budget,
        "token_budget": goal.token_budget,
        "created_at": goal.created_at,
        "completed_at": goal.completed_at,
        "remaining_tokens": remaining_tokens,
        "remaining": {
            "turns": remaining(goal.turn_budget, goal.turns_used),
            "tokens": remaining_tokens,
            "wall_clock_ms": remaining(goal.wall_clock_budget_ms, goal.wall_clock_ms),
        },
        "completion_budget_report": completion_budget_report(goal),
    }))
}

fn completion_budget_report(goal: &Goal) -> Value {
    if goal.status == GoalStatus::Complete {
        json!({
            "tokens_used": goal.tokens_used,
            "turns_used": goal.turns_used,
            "wall_clock_ms": goal.wall_clock_ms,
        })
    } else {
        Value::Null
    }
}

fn remaining(budget: i64, used: i64) -> Value {
    if budget <= 0 {
        Value::Null
    } else {
        json!(budget.saturating_sub(used))
    }
}
