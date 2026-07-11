//! Context token estimation and cut-point selection.

use elph_ai::{AssistantContentBlock, Message, StopReason, Usage};
use serde_json::Value;

pub use crate::harness::types::CompactionSettings;

use crate::session::types::SessionTreeEntry;
use crate::types::{AgentMessage, CustomAgentMessage};

const ESTIMATED_IMAGE_CHARS: usize = 4800;

fn safe_json_stringify(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[unserializable]".to_string())
}

/// Calculate total context tokens from provider usage.
pub fn calculate_context_tokens(usage: &Usage) -> u64 {
    if usage.total_tokens > 0 {
        usage.total_tokens
    } else {
        usage.input + usage.output + usage.cache_read + usage.cache_write
    }
}

fn get_assistant_usage(msg: &AgentMessage) -> Option<&Usage> {
    let AgentMessage::Llm(llm) = msg else {
        return None;
    };
    let Message::Assistant(assistant) = llm.as_ref() else {
        return None;
    };
    if matches!(assistant.stop_reason, StopReason::Aborted | StopReason::Error) {
        return None;
    }
    let tokens = calculate_context_tokens(&assistant.usage);
    if tokens > 0 { Some(&assistant.usage) } else { None }
}

/// Return usage from the last valid assistant message in session entries.
pub fn get_last_assistant_usage(entries: &[SessionTreeEntry]) -> Option<Usage> {
    for entry in entries.iter().rev() {
        let SessionTreeEntry::Message { message, .. } = entry else {
            continue;
        };
        if let Some(usage) = get_assistant_usage(message) {
            return Some(usage.clone());
        }
    }
    None
}

/// Estimated context-token usage for a message list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextUsageEstimate {
    pub tokens: u64,
    pub usage_tokens: u64,
    pub trailing_tokens: u64,
    pub last_usage_index: Option<usize>,
}

fn message_timestamp(message: &AgentMessage) -> i64 {
    match message {
        AgentMessage::Llm(llm) => match llm.as_ref() {
            Message::User { timestamp, .. }
            | Message::ToolResult { timestamp, .. }
            | Message::Assistant(elph_ai::AssistantMessage { timestamp, .. }) => *timestamp,
        },
        AgentMessage::Custom(custom) => match custom {
            CustomAgentMessage::BashExecution { timestamp, .. }
            | CustomAgentMessage::BranchSummary { timestamp, .. }
            | CustomAgentMessage::CompactionSummary { timestamp, .. }
            | CustomAgentMessage::Custom { timestamp, .. } => *timestamp,
        },
    }
}

/// Prefer the last valid assistant usage that still describes the current prefix.
///
/// A newer prefix message (e.g. compaction summary) with a later timestamp
/// invalidates earlier assistant usage (#6464).
fn get_last_assistant_usage_info(messages: &[AgentMessage]) -> Option<(Usage, usize)> {
    let mut latest_prefix_timestamp = i64::MIN;
    let mut usage_info: Option<(Usage, usize)> = None;

    for (index, message) in messages.iter().enumerate() {
        if let Some(usage) = get_assistant_usage(message) {
            let ts = message_timestamp(message);
            if ts >= latest_prefix_timestamp {
                usage_info = Some((usage.clone(), index));
            }
        }
        latest_prefix_timestamp = latest_prefix_timestamp.max(message_timestamp(message));
    }

    usage_info
}

/// Estimate context tokens for messages using provider usage when available.
pub fn estimate_context_tokens(messages: &[AgentMessage]) -> ContextUsageEstimate {
    if let Some((usage, index)) = get_last_assistant_usage_info(messages) {
        let usage_tokens = calculate_context_tokens(&usage);
        let trailing_tokens: u64 = messages[index + 1..].iter().map(estimate_tokens).sum();
        ContextUsageEstimate {
            tokens: usage_tokens + trailing_tokens,
            usage_tokens,
            trailing_tokens,
            last_usage_index: Some(index),
        }
    } else {
        let estimated: u64 = messages.iter().map(estimate_tokens).sum();
        ContextUsageEstimate {
            tokens: estimated,
            usage_tokens: 0,
            trailing_tokens: estimated,
            last_usage_index: None,
        }
    }
}

/// Return whether context usage exceeds the configured compaction threshold.
pub fn should_compact(context_tokens: u64, context_window: u64, settings: CompactionSettings) -> bool {
    if !settings.enabled {
        return false;
    }
    context_tokens > context_window.saturating_sub(settings.reserve_tokens)
}

fn estimate_text_and_image_content_chars(content: &str) -> usize {
    content.chars().count()
}

fn estimate_blocks_chars(blocks: &[elph_ai::ContentBlock]) -> usize {
    let mut chars = 0usize;
    for block in blocks {
        match block {
            elph_ai::ContentBlock::Text { text } => chars += text.chars().count(),
            elph_ai::ContentBlock::Image { .. } => chars += ESTIMATED_IMAGE_CHARS,
        }
    }
    chars
}

/// Estimate token count for one message using a conservative character heuristic.
pub fn estimate_tokens(message: &AgentMessage) -> u64 {
    let chars = match message {
        AgentMessage::Llm(llm) => match llm.as_ref() {
            Message::User { content, .. } => match content {
                elph_ai::UserContent::Text(text) => estimate_text_and_image_content_chars(text),
                elph_ai::UserContent::Blocks(blocks) => estimate_blocks_chars(blocks),
            },
            Message::Assistant(assistant) => {
                let mut chars = 0usize;
                for block in &assistant.content {
                    match block {
                        AssistantContentBlock::Text(text) => chars += text.text.chars().count(),
                        AssistantContentBlock::Thinking(thinking) => {
                            chars += thinking.thinking.chars().count();
                        }
                        AssistantContentBlock::ToolCall(tool_call) => {
                            chars += tool_call.name.chars().count()
                                + safe_json_stringify(&tool_call.arguments).chars().count();
                        }
                    }
                }
                chars
            }
            Message::ToolResult { content, .. } => estimate_blocks_chars(content),
        },
        AgentMessage::Custom(CustomAgentMessage::BashExecution { command, output, .. }) => {
            command.chars().count() + output.as_ref().map(|s| s.chars().count()).unwrap_or(0)
        }
        AgentMessage::Custom(CustomAgentMessage::BranchSummary { summary, .. })
        | AgentMessage::Custom(CustomAgentMessage::CompactionSummary { summary, .. }) => summary.chars().count(),
        AgentMessage::Custom(CustomAgentMessage::Custom { content, .. }) => content
            .as_str()
            .map(estimate_text_and_image_content_chars)
            .unwrap_or_else(|| content.to_string().chars().count()),
    };
    chars.div_ceil(4) as u64
}

fn find_valid_cut_points(entries: &[SessionTreeEntry], start_index: usize, end_index: usize) -> Vec<usize> {
    let mut cut_points = Vec::new();
    for (i, entry) in entries.iter().enumerate().take(end_index).skip(start_index) {
        match entry {
            SessionTreeEntry::Message { message, .. } => match message.role() {
                "bashExecution" | "custom" | "branchSummary" | "compactionSummary" | "user" | "assistant" => {
                    cut_points.push(i);
                }
                _ => {}
            },
            SessionTreeEntry::BranchSummary { .. } | SessionTreeEntry::CustomMessage { .. } => {
                cut_points.push(i);
            }
            _ => {}
        }
    }
    cut_points
}

/// Find the user-visible message that starts the turn containing an entry.
pub fn find_turn_start_index(entries: &[SessionTreeEntry], entry_index: usize, start_index: usize) -> Option<usize> {
    for i in (start_index..=entry_index).rev() {
        match &entries[i] {
            SessionTreeEntry::BranchSummary { .. } | SessionTreeEntry::CustomMessage { .. } => return Some(i),
            SessionTreeEntry::Message { message, .. } => match message.role() {
                "user" | "bashExecution" => return Some(i),
                _ => {}
            },
            _ => {}
        }
    }
    None
}

/// Cut point selected for compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutPointResult {
    pub first_kept_entry_index: usize,
    pub turn_start_index: Option<usize>,
    pub is_split_turn: bool,
}

/// Find the compaction cut point that keeps approximately the requested recent-token budget.
pub fn find_cut_point(
    entries: &[SessionTreeEntry],
    start_index: usize,
    end_index: usize,
    keep_recent_tokens: u64,
) -> CutPointResult {
    let cut_points = find_valid_cut_points(entries, start_index, end_index);
    if cut_points.is_empty() {
        return CutPointResult {
            first_kept_entry_index: start_index,
            turn_start_index: None,
            is_split_turn: false,
        };
    }

    let mut accumulated_tokens = 0u64;
    let mut cut_index = cut_points[0];

    for i in (start_index..end_index).rev() {
        let SessionTreeEntry::Message { message, .. } = &entries[i] else {
            continue;
        };
        accumulated_tokens += estimate_tokens(message);
        if accumulated_tokens >= keep_recent_tokens {
            for &candidate in &cut_points {
                if candidate >= i {
                    cut_index = candidate;
                    break;
                }
            }
            break;
        }
    }

    while cut_index > start_index {
        match &entries[cut_index - 1] {
            SessionTreeEntry::Compaction { .. } | SessionTreeEntry::Message { .. } => break,
            _ => cut_index -= 1,
        }
    }

    let is_user_message = matches!(
        &entries[cut_index],
        SessionTreeEntry::Message { message, .. } if message.role() == "user"
    );
    let turn_start_index = if is_user_message {
        None
    } else {
        find_turn_start_index(entries, cut_index, start_index)
    };

    CutPointResult {
        first_kept_entry_index: cut_index,
        turn_start_index,
        is_split_turn: !is_user_message && turn_start_index.is_some(),
    }
}
