//! `/goal` slash command handler.

use anyhow::{Context, Result};
use elph_agent::{GoalRuntime, GoalStatus};

pub async fn handle_goal_slash(goal_runtime: &GoalRuntime, args: &str) -> Result<String> {
    let store = goal_runtime.store();
    let session_id = goal_runtime.session_id();
    let trimmed = args.trim();

    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("status") {
        return format_goal(store.get_latest_goal(session_id).await?);
    }

    let mut parts = trimmed.splitn(2, ' ');
    let sub = parts.next().unwrap_or("").to_ascii_lowercase();
    let rest = parts.next().unwrap_or("").trim();

    match sub.as_str() {
        "pause" => {
            let goal = store.set_status(session_id, GoalStatus::Paused).await?;
            Ok(format!("Goal paused: {}", goal.objective))
        }
        "resume" => {
            let goal = store.resume_goal(session_id).await?;
            Ok(format!("Goal resumed: {}", goal.objective))
        }
        "cancel" => {
            store.clear_goal(session_id).await?;
            Ok("Goal cancelled.".into())
        }
        "replace" => {
            if rest.is_empty() {
                anyhow::bail!("usage: /goal replace <objective>");
            }
            let goal = store.replace_goal(session_id, rest, None, 0, 0, 0).await?;
            Ok(format!("Goal replaced: {}", goal.objective))
        }
        "next" => {
            if rest.is_empty() {
                anyhow::bail!("usage: /goal next <objective>");
            }
            anyhow::bail!("queued next goals are not implemented yet; use /goal replace")
        }
        _ => {
            let objective = trimmed;
            let goal = store.create_goal(session_id, objective, None, 0, 0, 0).await?;
            Ok(format!("Goal created: {}", goal.objective))
        }
    }
}

fn format_goal(goal: Option<elph_agent::Goal>) -> Result<String> {
    let Some(goal) = goal else {
        return Ok("No goal for this session.".into());
    };
    Ok(format!(
        "Goal: {}\nStatus: {}\nTurns: {} / {}\nTokens: {} / {}\nWall ms: {} / {}",
        goal.objective,
        goal.status.as_str(),
        goal.turns_used,
        display_budget(goal.turn_budget),
        goal.tokens_used,
        display_budget(goal.token_budget),
        goal.wall_clock_ms,
        display_budget(goal.wall_clock_budget_ms),
    ))
}

fn display_budget(budget: i64) -> String {
    if budget <= 0 { "∞".into() } else { budget.to_string() }
}

pub async fn handle_goal_slash_result(
    goal_runtime: &GoalRuntime,
    args: &str,
) -> Result<(String, Option<elph_agent::Goal>)> {
    let message = handle_goal_slash(goal_runtime, args).await?;
    let goal = goal_runtime
        .store()
        .get_latest_goal(goal_runtime.session_id())
        .await
        .context("load goal after slash")?;
    Ok((message, goal))
}
