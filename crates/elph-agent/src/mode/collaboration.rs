//! Collaboration mode — Codex-style Plan vs Default execution.

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
