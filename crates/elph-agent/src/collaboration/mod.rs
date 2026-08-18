//! Collaboration modes (Plan / Default) and planning helpers.

mod plan;
mod policy;
mod tracker;

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Session collaboration mode (distinct from product agent mode labels).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollaborationMode {
    /// Full tool access — build / execute.
    #[default]
    Default,
    /// Planning — exploration plus per-call mutating workspace tools; implement still needs plan confirmation.
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

pub use plan::PlanConfirmationChoice;
pub use plan::{assistant_message_text, extract_proposed_plan, implement_prompt};
pub use policy::ToolExposurePolicy;
pub use policy::default_exploration_tools;
pub use policy::filter_active_tools;
pub use policy::is_collaboration_tool;
pub use policy::is_exploration_tool;
pub use policy::is_mcp_read_only_bridge_tool;
pub use policy::is_mcp_tool;
pub use policy::is_mutating_tool;
pub use policy::is_plan_exposed_tool;
pub use policy::is_plan_mode_tool;
pub use policy::is_read_only_mcp_tool;
pub use policy::plan_mode_block_reason;
pub use policy::plan_mode_blocks_tool;
pub use tracker::{PlanModeState, PlanModeTracker};
