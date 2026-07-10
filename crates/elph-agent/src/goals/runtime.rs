//! Goal lifecycle integration for the agent harness.

use std::sync::Arc;

use anyhow::Result;
use elph_ai::Usage;
use tokio::sync::Mutex;

use super::accounting::GoalAccountingState;
use super::steering::{budget_limit_prompt, continuation_prompt};
use super::store::GoalStore;
use super::types::{Goal, GoalStatus};
use crate::mode::CollaborationMode;

#[derive(Debug, Clone)]
pub enum GoalTurnStart {
    Ok,
    Blocked(String),
}

#[derive(Debug, Clone)]
pub enum GoalTurnFinish {
    None,
    BudgetLimited(Goal),
    Continuation(Goal),
}

pub struct GoalRuntime {
    store: Arc<GoalStore>,
    session_id: String,
    accounting: Mutex<GoalAccountingState>,
}

impl GoalRuntime {
    pub fn new(store: Arc<GoalStore>, session_id: impl Into<String>) -> Self {
        Self {
            store,
            session_id: session_id.into(),
            accounting: Mutex::new(GoalAccountingState::default()),
        }
    }

    pub fn store(&self) -> Arc<GoalStore> {
        self.store.clone()
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub async fn start_turn(&self, mode: CollaborationMode) -> Result<GoalTurnStart> {
        if mode == CollaborationMode::Plan {
            return Ok(GoalTurnStart::Ok);
        }

        let goal = self.store.get_latest_goal(&self.session_id).await?;
        let Some(goal) = goal else {
            return Ok(GoalTurnStart::Ok);
        };

        if goal.status.blocks_turns() {
            return Ok(GoalTurnStart::Blocked(format!(
                "Goal is {} — use /goal resume or extend budgets before continuing.",
                goal.status.as_str()
            )));
        }

        if goal.status != GoalStatus::Active {
            return Ok(GoalTurnStart::Ok);
        }

        let mut accounting = self.accounting.lock().await;
        accounting.start_turn(goal.tokens_used, goal.wall_clock_ms);
        Ok(GoalTurnStart::Ok)
    }

    pub async fn finish_turn(&self, mode: CollaborationMode, usage: Option<&Usage>) -> Result<GoalTurnFinish> {
        if mode == CollaborationMode::Plan {
            return Ok(GoalTurnFinish::None);
        }

        let accounting = self.accounting.lock().await;
        let (token_delta, turn_delta, wall_delta) = accounting.finish_turn(usage);
        drop(accounting);

        let updated = self
            .store
            .record_usage(&self.session_id, token_delta, turn_delta, wall_delta)
            .await?;

        let Some(goal) = updated else {
            return Ok(GoalTurnFinish::None);
        };

        if goal.status == GoalStatus::BudgetLimited {
            return Ok(GoalTurnFinish::BudgetLimited(goal));
        }

        if goal.status == GoalStatus::Active {
            return Ok(GoalTurnFinish::Continuation(goal));
        }

        Ok(GoalTurnFinish::None)
    }

    pub fn budget_steering(goal: &Goal) -> String {
        budget_limit_prompt(goal)
    }

    pub fn continuation_steering(goal: &Goal) -> String {
        continuation_prompt(goal)
    }
}
