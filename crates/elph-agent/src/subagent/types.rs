//! Subagent types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Pending,
    Running,
    Idle,
    Error,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentInfo {
    pub id: String,
    pub task_name: String,
    pub status: SubagentStatus,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubagentLimits {
    pub max_depth: u32,
    pub max_concurrent: usize,
}

impl Default for SubagentLimits {
    fn default() -> Self {
        Self {
            max_depth: 3,
            max_concurrent: 4,
        }
    }
}
