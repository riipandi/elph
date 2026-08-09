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
    pub usage: TurnUsage,
    pub user_entry_id: Option<String>,
    pub assistant_entry_id: Option<String>,
    pub error_message: Option<String>,
}
