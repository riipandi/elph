//! Goal continuation and budget steering prompts.

use crate::goals::types::Goal;

/// Prefix of [`continuation_prompt`] — used by the TUI to detect and hide these
/// internal steering prompts from the transcript (render as a meta label instead).
pub const CONTINUATION_PROMPT_PREFIX: &str = "Continue working on the active goal.";

/// Prefix of [`budget_limit_prompt`] — used by the TUI to detect and hide these
/// internal steering prompts from the transcript.
pub const BUDGET_LIMIT_PROMPT_PREFIX: &str = "The session goal has reached its budget limit.";

pub fn continuation_prompt(goal: &Goal) -> String {
    format!(
        "Continue working on the active goal.\n\n\
         Objective: {}\n\
         Status: {}\n\
         Turns used: {} / {}\n\
         Tokens used: {} / {}\n\
         Wall clock ms: {} / {}\n\n\
         Make measurable progress toward completion. \
         Call update_goal with status \"complete\" when done, or \"blocked\" if stuck.",
        goal.objective,
        goal.status.as_str(),
        goal.turns_used,
        display_budget(goal.turn_budget),
        goal.tokens_used,
        display_budget(goal.token_budget),
        goal.wall_clock_ms,
        display_budget(goal.wall_clock_budget_ms),
    )
}

pub fn budget_limit_prompt(goal: &Goal) -> String {
    format!(
        "The session goal has reached its budget limit.\n\n\
         Objective: {}\n\
         Tokens used: {} / {}\n\
         Turns used: {} / {}\n\
         Wall clock ms: {} / {}\n\n\
         Pause and ask the user to extend the budget via /goal or set_goal_budget, \
         or mark the goal complete/blocked with update_goal.",
        goal.objective,
        goal.tokens_used,
        display_budget(goal.token_budget),
        goal.turns_used,
        display_budget(goal.turn_budget),
        goal.wall_clock_ms,
        display_budget(goal.wall_clock_budget_ms),
    )
}

fn display_budget(budget: i64) -> String {
    if budget <= 0 {
        "unlimited".into()
    } else {
        budget.to_string()
    }
}
