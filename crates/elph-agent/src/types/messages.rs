//! Agent transcript message types and conversions.

use elph_ai::{AssistantMessage, Message, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// App-level transcript message (LLM messages + custom roles).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentMessage {
    Llm(Box<Message>),
    Custom(CustomAgentMessage),
}

impl AgentMessage {
    pub fn role(&self) -> &str {
        match self {
            Self::Llm(m) => m.role(),
            Self::Custom(c) => c.role(),
        }
    }

    pub fn as_llm(&self) -> Option<&Message> {
        match self {
            Self::Llm(m) => Some(m.as_ref()),
            _ => None,
        }
    }

    pub fn into_llm(self) -> Option<Message> {
        match self {
            Self::Llm(m) => Some(*m),
            _ => None,
        }
    }
}

/// Custom harness message roles (extended in `messages` module).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum CustomAgentMessage {
    BashExecution {
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<String>,
        #[serde(rename = "exitCode", default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(default)]
        cancelled: bool,
        #[serde(default)]
        truncated: bool,
        #[serde(rename = "fullOutputPath", default, skip_serializing_if = "Option::is_none")]
        full_output_path: Option<String>,
        timestamp: i64,
        #[serde(rename = "excludeFromContext", default, skip_serializing_if = "std::ops::Not::not")]
        exclude_from_context: bool,
    },
    BranchSummary {
        summary: String,
        #[serde(rename = "fromId")]
        from_id: String,
        timestamp: i64,
    },
    CompactionSummary {
        summary: String,
        #[serde(rename = "tokensBefore")]
        tokens_before: u64,
        timestamp: i64,
    },
    Custom {
        #[serde(rename = "type")]
        kind: String,
        content: Value,
        #[serde(default)]
        display: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        timestamp: i64,
    },
}

impl CustomAgentMessage {
    pub fn role(&self) -> &str {
        match self {
            Self::BashExecution { .. } => "bashExecution",
            Self::BranchSummary { .. } => "branchSummary",
            Self::CompactionSummary { .. } => "compactionSummary",
            Self::Custom { .. } => "custom",
        }
    }
}

pub fn assistant_message_to_agent(message: AssistantMessage) -> AgentMessage {
    AgentMessage::Llm(Box::new(Message::Assistant(message)))
}

pub fn tool_result_to_agent(message: Message) -> AgentMessage {
    AgentMessage::Llm(Box::new(message))
}

pub fn llm_message_to_agent(message: Message) -> AgentMessage {
    AgentMessage::Llm(Box::new(message))
}

pub fn extract_tool_calls(message: &AssistantMessage) -> Vec<&ToolCall> {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            elph_ai::AssistantContentBlock::ToolCall(tc) => Some(tc),
            _ => None,
        })
        .collect()
}
