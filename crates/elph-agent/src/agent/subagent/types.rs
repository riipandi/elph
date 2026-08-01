//! Subagent types.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::agent::harness::{AgentHarnessResources, AgentHarnessStreamOptions};
use crate::agent::subagent::graph::AgentGraphStore;
use crate::prompt::encoding::PromptEncodingConfig;
use crate::types::AgentThinkingLevel;

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
    pub session_id: String,
    pub task_name: String,
    pub agent_path: String,
    pub depth: u32,
    pub status: SubagentStatus,
    pub parent_session_id: String,
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

/// Shared bootstrap data for spawning child session harnesses.
#[derive(Clone)]
pub struct SubagentBootstrap {
    pub cwd: String,
    /// Shared platform/session database (`APP_DATA/metadata.db` in the product).
    pub metadata_db_path: String,
    pub resources: AgentHarnessResources,
    pub stream_options: AgentHarnessStreamOptions,
    pub thinking_level: AgentThinkingLevel,
    /// TOON prompt-encoding config inherited from the parent; `None` falls back to env.
    pub prompt_encoding: Option<PromptEncodingConfig>,
    pub agent_graph: Option<Arc<AgentGraphStore>>,
}
