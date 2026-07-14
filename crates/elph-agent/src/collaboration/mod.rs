//! Collaboration modes (Plan / Default) and planning helpers.

mod plan;
mod policy;

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Session collaboration mode (distinct from TUI `AgentMode` labels).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollaborationMode {
    /// Full tool access — build / execute.
    #[default]
    Default,
    /// Read-only planning — no mutating tools.
    Plan,
}

impl CollaborationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Plan => "plan",
        }
    }
}

impl FromStr for CollaborationMode {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value.trim().to_ascii_lowercase().as_str() {
            "plan" => Self::Plan,
            _ => Self::Default,
        })
    }
}

pub use plan::{PlanConfirmationChoice, assistant_message_text, extract_proposed_plan, implement_prompt};
pub use policy::{
    filter_active_tools, is_collaboration_tool, is_mcp_read_only_bridge_tool, is_mcp_tool, is_mutating_tool,
    is_plan_mode_tool, plan_mode_block_reason, plan_mode_blocks_tool, plan_mode_system_prompt,
};
