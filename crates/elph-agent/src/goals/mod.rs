//! Session goal persistence and agent tools.

mod accounting;
#[cfg(feature = "backend-turso")]
mod runtime;
mod steering;
#[cfg(feature = "backend-turso")]
mod store;
#[cfg(feature = "backend-turso")]
mod tools;
mod types;

pub use accounting::GoalAccountingState;
pub use accounting::goal_token_delta;
#[cfg(feature = "backend-turso")]
pub use runtime::{GoalRuntime, GoalTurnFinish, GoalTurnStart};
pub use steering::{BUDGET_LIMIT_PROMPT_PREFIX, CONTINUATION_PROMPT_PREFIX};
#[cfg(feature = "backend-turso")]
pub use store::GoalStore;
#[cfg(feature = "backend-turso")]
pub use tools::{GoalStatusHook, create_goal_tools, create_goal_tools_with_hook};
pub use types::{Goal, GoalStatus};
