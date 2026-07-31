//! Session goal persistence and agent tools.

mod accounting;
mod runtime;
mod steering;
mod store;
mod tools;
mod types;

pub use accounting::GoalAccountingState;
pub use accounting::goal_token_delta;
pub use runtime::{GoalRuntime, GoalTurnFinish, GoalTurnStart};
pub use store::GoalStore;
pub use tools::{GoalStatusHook, create_goal_tools, create_goal_tools_with_hook};
pub use types::{Goal, GoalStatus};
