//! Message bridge — `convert_to_llm` and custom role handling.

pub mod types;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::{ImageContent, Message, TextContent, UserContent};

use crate::types::{AgentMessage, ConvertToLlmFn, CustomAgentMessage};

pub const COMPACTION_SUMMARY_PREFIX: &str =
    "The conversation history before this point was compacted into the following summary:\n\n<summary>\n";
pub const COMPACTION_SUMMARY_SUFFIX: &str = "\n</summary>";
pub const BRANCH_SUMMARY_PREFIX: &str =
    "The following is a summary of a branch that this conversation came back from:\n\n<summary>\n";
pub const BRANCH_SUMMARY_SUFFIX: &str = "</summary>";

fn iso_to_millis(timestamp: &str) -> i64 {
    time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
        .or_else(|_| time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Iso8601::DEFAULT))
        .map(|dt| (dt.unix_timestamp_nanos() / 1_000_000) as i64)
        .unwrap_or(0)
}

pub fn now_iso_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Return the current local date with its UTC offset, formatted as
/// `YYYY-MM-DD UTC±H` (e.g. `2026-08-16 UTC+7`). Falls back to UTC when
/// the system timezone is unavailable.
pub fn now_date_with_offset() -> String {
    let dt = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let offset = dt.offset();
    let hours = offset.whole_hours();
    let tz = if hours == 0 {
        "UTC".to_string()
    } else if hours > 0 {
        format!("UTC+{}", hours)
    } else {
        format!("UTC{}", hours)
    };
    format!(
        "{} {}",
        dt.format(&time::format_description::well_known::Iso8601::DATE)
            .unwrap_or_default(),
        tz
    )
}

pub fn create_branch_summary_message(summary: &str, from_id: &str, timestamp: &str) -> AgentMessage {
    AgentMessage::Custom(CustomAgentMessage::BranchSummary {
        summary: summary.to_string(),
        from_id: from_id.to_string(),
        timestamp: iso_to_millis(timestamp),
    })
}

pub fn create_compaction_summary_message(summary: &str, tokens_before: u64, timestamp: &str) -> AgentMessage {
    AgentMessage::Custom(CustomAgentMessage::CompactionSummary {
        summary: summary.to_string(),
        tokens_before,
        timestamp: iso_to_millis(timestamp),
    })
}

pub fn create_custom_message(
    custom_type: &str,
    content: CustomMessageContent,
    display: bool,
    details: Option<serde_json::Value>,
    timestamp: &str,
) -> AgentMessage {
    let value = match content {
        CustomMessageContent::Text(text) => serde_json::Value::String(text),
        CustomMessageContent::Blocks(blocks) => serde_json::to_value(blocks).unwrap_or_default(),
    };
    AgentMessage::Custom(CustomAgentMessage::Custom {
        kind: custom_type.to_string(),
        content: value,
        display,
        details,
        timestamp: iso_to_millis(timestamp),
    })
}

#[derive(Debug, Clone)]
pub enum CustomMessageContent {
    Text(String),
    Blocks(Vec<CustomMessageBlock>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum CustomMessageBlock {
    Text(TextContent),
    Image(ImageContent),
}

/// Format a shell_exec custom message for LLM context (pi-agent shellExecExecutionToText parity).
pub fn shell_exec_execution_to_text(msg: &CustomAgentMessage) -> Option<String> {
    let CustomAgentMessage::ShellExecExecution {
        command,
        output,
        exit_code,
        cancelled,
        truncated,
        full_output_path,
        ..
    } = msg
    else {
        return None;
    };

    let mut text = format!("Ran `{command}`\n");
    if let Some(out) = output.as_deref().filter(|out| !out.is_empty()) {
        text.push_str(&format!("```\n{out}\n```"));
    } else {
        text.push_str("(no output)");
    }
    if *cancelled {
        text.push_str("\n\n(command cancelled)");
    } else if let Some(code) = exit_code.filter(|&code| code != 0) {
        text.push_str(&format!("\n\nCommand exited with code {code}"));
    }
    if *truncated && let Some(path) = full_output_path {
        text.push_str(&format!("\n\n[Output truncated. Full output: {path}]"));
    }
    Some(text)
}

/// Default conversion: keep user, assistant, and tool-result LLM messages.
pub fn default_convert_to_llm(messages: Vec<AgentMessage>) -> Vec<Message> {
    messages
        .into_iter()
        .filter_map(|message| match message {
            AgentMessage::Llm(m) if matches!(m.role(), "user" | "assistant" | "toolResult") => Some(*m),
            AgentMessage::Custom(CustomAgentMessage::ShellExecExecution {
                exclude_from_context: true,
                ..
            }) => None,
            AgentMessage::Custom(msg @ CustomAgentMessage::ShellExecExecution { timestamp, .. }) => {
                shell_exec_execution_to_text(&msg).map(|text| Message::User {
                    content: elph_ai::UserContent::Text(text),
                    timestamp,
                })
            }
            AgentMessage::Custom(CustomAgentMessage::BranchSummary { summary, timestamp, .. }) => Some(Message::User {
                content: UserContent::Text(format!("{BRANCH_SUMMARY_PREFIX}{summary}{BRANCH_SUMMARY_SUFFIX}")),
                timestamp,
            }),
            AgentMessage::Custom(CustomAgentMessage::CompactionSummary { summary, timestamp, .. }) => {
                Some(Message::User {
                    content: UserContent::Text(format!(
                        "{COMPACTION_SUMMARY_PREFIX}{summary}{COMPACTION_SUMMARY_SUFFIX}"
                    )),
                    timestamp,
                })
            }
            AgentMessage::Custom(CustomAgentMessage::Custom { content, timestamp, .. }) => {
                let text = content
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| content.to_string());
                Some(Message::User {
                    content: UserContent::Text(text),
                    timestamp,
                })
            }
            _ => None,
        })
        .collect()
}

pub fn default_convert_to_llm_fn() -> ConvertToLlmFn {
    Arc::new(|messages| Box::pin(async move { default_convert_to_llm(messages) }))
}

pub fn convert_to_llm_sync(messages: Vec<AgentMessage>) -> Pin<Box<dyn Future<Output = Vec<Message>> + Send>> {
    Box::pin(async move { default_convert_to_llm(messages) })
}
