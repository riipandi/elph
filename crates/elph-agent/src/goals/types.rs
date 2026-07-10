//! Goal domain types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Complete,
    Blocked,
    Paused,
    BudgetLimited,
    UsageLimited,
}

impl GoalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Complete => "complete",
            Self::Blocked => "blocked",
            Self::Paused => "paused",
            Self::BudgetLimited => "budget_limited",
            Self::UsageLimited => "usage_limited",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "complete" => Some(Self::Complete),
            "blocked" => Some(Self::Blocked),
            "paused" => Some(Self::Paused),
            "budget_limited" => Some(Self::BudgetLimited),
            "usage_limited" => Some(Self::UsageLimited),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Blocked)
    }

    pub fn blocks_turns(self) -> bool {
        matches!(self, Self::BudgetLimited | Self::UsageLimited | Self::Paused)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Goal {
    pub id: i64,
    pub goal_id: String,
    pub session_id: String,
    pub objective: String,
    pub completion_criterion: Option<String>,
    pub status: GoalStatus,
    pub turns_used: i64,
    pub tokens_used: i64,
    pub wall_clock_ms: i64,
    pub wall_clock_budget_ms: i64,
    pub turn_budget: i64,
    pub token_budget: i64,
    pub created_at: String,
    pub completed_at: Option<String>,
}
