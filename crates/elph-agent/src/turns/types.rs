//! Turn domain types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Started,
    Completed,
    Failed,
    Interrupted,
}

impl TurnStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "started" => Some(Self::Started),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TurnUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_tokens: i64,
    pub cost: f64,
}

impl TurnUsage {
    pub fn from_ai_usage(usage: &elph_ai::Usage) -> Self {
        Self {
            input_tokens: usage.input as i64,
            output_tokens: usage.output as i64,
            cache_read_tokens: usage.cache_read as i64,
            cache_write_tokens: usage.cache_write as i64,
            total_tokens: usage.total_tokens as i64,
            cost: usage.cost.total,
        }
    }
}

impl std::ops::AddAssign for TurnUsage {
    fn add_assign(&mut self, rhs: Self) {
        self.input_tokens += rhs.input_tokens;
        self.output_tokens += rhs.output_tokens;
        self.cache_read_tokens += rhs.cache_read_tokens;
        self.cache_write_tokens += rhs.cache_write_tokens;
        self.total_tokens += rhs.total_tokens;
        self.cost += rhs.cost;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnRecord {
    pub id: String,
    pub session_id: String,
    pub turn_index: i64,
    pub status: TurnStatus,
    pub operation_id: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub wall_clock_ms: i64,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub thinking_level: Option<String>,
    /// Agent mode at turn start (`build` / `plan` / `ask` / `brave`), when known.
    pub agent_mode: Option<String>,
    pub usage: TurnUsage,
    pub user_entry_id: Option<String>,
    pub assistant_entry_id: Option<String>,
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::TurnUsage;

    #[test]
    fn add_assign_sums_all_fields() {
        let mut acc = TurnUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 10,
            cache_write_tokens: 5,
            total_tokens: 165,
            cost: 0.01,
        };
        acc += TurnUsage {
            input_tokens: 200,
            output_tokens: 25,
            cache_read_tokens: 0,
            cache_write_tokens: 40,
            total_tokens: 265,
            cost: 0.02,
        };
        assert_eq!(acc.input_tokens, 300);
        assert_eq!(acc.output_tokens, 75);
        assert_eq!(acc.cache_read_tokens, 10);
        assert_eq!(acc.cache_write_tokens, 45);
        assert_eq!(acc.total_tokens, 430);
        assert!((acc.cost - 0.03).abs() < 1e-9);
    }
}
