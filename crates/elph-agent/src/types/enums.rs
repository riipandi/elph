//! Agent loop enumerations.

use elph_ai::ThinkingLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolExecutionMode {
    Sequential,
    #[default]
    Parallel,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QueueMode {
    All,
    #[default]
    #[serde(rename = "one-at-a-time")]
    OneAtATime,
}

/// Thinking level including harness-only `Off`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl AgentThinkingLevel {
    pub fn to_stream_reasoning(self) -> Option<ThinkingLevel> {
        match self {
            Self::Off => None,
            Self::Minimal => Some(ThinkingLevel::Minimal),
            Self::Low => Some(ThinkingLevel::Low),
            Self::Medium => Some(ThinkingLevel::Medium),
            Self::High => Some(ThinkingLevel::High),
            Self::Xhigh => Some(ThinkingLevel::Xhigh),
            Self::Max => Some(ThinkingLevel::Max),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_level_off_returns_none() {
        assert_eq!(AgentThinkingLevel::Off.to_stream_reasoning(), None);
    }

    #[test]
    fn thinking_level_all_variants_map() {
        assert_eq!(AgentThinkingLevel::Minimal.to_stream_reasoning(), Some(ThinkingLevel::Minimal));
        assert_eq!(AgentThinkingLevel::Low.to_stream_reasoning(), Some(ThinkingLevel::Low));
        assert_eq!(AgentThinkingLevel::Medium.to_stream_reasoning(), Some(ThinkingLevel::Medium));
        assert_eq!(AgentThinkingLevel::High.to_stream_reasoning(), Some(ThinkingLevel::High));
        assert_eq!(AgentThinkingLevel::Xhigh.to_stream_reasoning(), Some(ThinkingLevel::Xhigh));
        assert_eq!(AgentThinkingLevel::Max.to_stream_reasoning(), Some(ThinkingLevel::Max));
    }

    #[test]
    fn tool_execution_mode_default_is_parallel() {
        assert_eq!(ToolExecutionMode::default(), ToolExecutionMode::Parallel);
    }

    #[test]
    fn queue_mode_default_is_one_at_a_time() {
        assert_eq!(QueueMode::default(), QueueMode::OneAtATime);
    }

    #[test]
    fn queue_mode_serialization() {
        let mode = QueueMode::OneAtATime;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"one-at-a-time\"");
        let parsed: QueueMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, QueueMode::OneAtATime);
    }

    #[test]
    fn tool_execution_mode_serialization() {
        let mode = ToolExecutionMode::Sequential;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"sequential\"");
        let parsed: ToolExecutionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ToolExecutionMode::Sequential);
    }
}
