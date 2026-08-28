//! Context token estimation and cut-point selection.

use elph_ai::estimate::count_tokens_text;
use elph_ai::{AssistantContentBlock, Message, StopReason, Usage};
use serde_json::Value;

pub use crate::agent::harness::types::CompactionSettings;

use crate::session::types::SessionTreeEntry;
use crate::types::{AgentMessage, CustomAgentMessage};

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
            CustomAgentMessage::ShellExecExecution { timestamp, .. }
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

/// Return whether context usage reaches the configured compaction threshold.
pub fn should_compact(context_tokens: u64, context_window: u64, settings: CompactionSettings) -> bool {
    if !settings.enabled || context_window == 0 {
        return false;
    }
    let threshold = match settings.threshold_pct {
        Some(pct) => context_window.saturating_mul(pct as u64) / 100,
        None => context_window.saturating_sub(settings.reserve_tokens),
    };
    context_tokens >= threshold
}

/// Combine a message-level context estimate with the compiled system prompt.
///
/// The system prompt is added only when the estimate did **not** reuse provider
/// usage (`last_usage_index == None`) — i.e. an empty/early history, or a history
/// rewritten by a compaction summary. Provider `usage.total_tokens` for an
/// assistant turn already includes the system prompt that was sent with that
/// request, so adding it again double-counts and inflates the displayed usage
/// (it can even read past 100% of the context window while the real next request
/// still fits). When usage is reused we take the provider number as-is.
pub fn estimate_tokens_with_system_prompt(estimate: ContextUsageEstimate, system_prompt: Option<&str>) -> u64 {
    let mut tokens = estimate.tokens;
    if estimate.last_usage_index.is_none()
        && let Some(sp) = system_prompt
    {
        tokens += count_tokens_text(sp);
    }
    tokens
}

// Image tokens estimated separately — not subject to the char-based heuristic.
const ESTIMATED_IMAGE_TOKENS: u64 = 1200;

/// Estimate token count for one message using the tokenx heuristic.
pub fn estimate_tokens(message: &AgentMessage) -> u64 {
    match message {
        AgentMessage::Llm(llm) => match llm.as_ref() {
            Message::User { content, .. } => match content {
                elph_ai::UserContent::Text(text) => count_tokens_text(text),
                elph_ai::UserContent::Blocks(blocks) => blocks
                    .iter()
                    .map(|b| match b {
                        elph_ai::ContentBlock::Text { text } => count_tokens_text(text),
                        elph_ai::ContentBlock::Image { .. } => ESTIMATED_IMAGE_TOKENS,
                    })
                    .sum(),
            },
            Message::Assistant(assistant) => assistant
                .content
                .iter()
                .map(|block| match block {
                    AssistantContentBlock::Text(text) => count_tokens_text(&text.text),
                    AssistantContentBlock::Thinking(thinking) => count_tokens_text(&thinking.thinking),
                    AssistantContentBlock::ToolCall(tool_call) => {
                        count_tokens_text(&tool_call.name)
                            + count_tokens_text(&tool_call.id)
                            + count_tokens_text(&safe_json_stringify(&tool_call.arguments))
                    }
                })
                .sum(),
            Message::ToolResult { content, .. } => content
                .iter()
                .map(|b| match b {
                    elph_ai::ContentBlock::Text { text } => count_tokens_text(text),
                    elph_ai::ContentBlock::Image { .. } => ESTIMATED_IMAGE_TOKENS,
                })
                .sum(),
        },
        AgentMessage::Custom(CustomAgentMessage::ShellExecExecution { command, output, .. }) => {
            let mut total = count_tokens_text(command);
            if let Some(out) = output.as_ref() {
                total += count_tokens_text(out);
            }
            total
        }
        AgentMessage::Custom(CustomAgentMessage::BranchSummary { summary, .. })
        | AgentMessage::Custom(CustomAgentMessage::CompactionSummary { summary, .. }) => count_tokens_text(summary),
        AgentMessage::Custom(CustomAgentMessage::Custom { content, .. }) => content
            .as_str()
            .map(count_tokens_text)
            .unwrap_or_else(|| count_tokens_text(&serde_json::to_string(content).unwrap_or_default())),
    }
}

fn find_valid_cut_points(entries: &[SessionTreeEntry], start_index: usize, end_index: usize) -> Vec<usize> {
    let mut cut_points = Vec::new();
    for (i, entry) in entries.iter().enumerate().take(end_index).skip(start_index) {
        match entry {
            SessionTreeEntry::Message { message, .. } => match message.role() {
                "shellExecExecution" | "custom" | "branchSummary" | "compactionSummary" | "user" | "assistant" => {
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
pub(crate) fn find_turn_start_index(
    entries: &[SessionTreeEntry],
    entry_index: usize,
    start_index: usize,
) -> Option<usize> {
    for i in (start_index..=entry_index).rev() {
        match &entries[i] {
            SessionTreeEntry::BranchSummary { .. } | SessionTreeEntry::CustomMessage { .. } => return Some(i),
            SessionTreeEntry::Message { message, .. } => match message.role() {
                "user" | "shellExecExecution" => return Some(i),
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
pub(crate) fn find_cut_point(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn estimate(tokens: u64, last_usage_index: Option<usize>) -> ContextUsageEstimate {
        ContextUsageEstimate {
            tokens,
            usage_tokens: if last_usage_index.is_some() { tokens } else { 0 },
            trailing_tokens: if last_usage_index.is_some() { 0 } else { tokens },
            last_usage_index,
        }
    }

    #[test]
    fn system_prompt_not_double_counted_when_usage_is_reused() {
        // Provider usage already includes the system prompt; adding it again must not happen.
        let estimate = estimate(203_211, Some(42));
        let total = estimate_tokens_with_system_prompt(estimate, Some("system prompt text"));
        assert_eq!(total, 203_211);
    }

    #[test]
    fn system_prompt_added_when_no_usage_reused() {
        // Empty history / post-compaction prefix: no provider usage, count system prompt once.
        let estimate = estimate(500, None);
        let total = estimate_tokens_with_system_prompt(estimate, Some("system prompt text"));
        assert!(total > 500, "system prompt should be added on top of the heuristic estimate");
    }

    #[test]
    fn no_system_prompt_leaves_estimate_unchanged() {
        let est = estimate(1234, Some(7));
        assert_eq!(estimate_tokens_with_system_prompt(est, None), 1234);
        let est = estimate(1234, None);
        assert_eq!(estimate_tokens_with_system_prompt(est, None), 1234);
    }

    fn user_message(text: &str) -> crate::types::AgentMessage {
        crate::types::AgentMessage::Llm(Box::new(elph_ai::Message::User {
            content: elph_ai::UserContent::Text(text.to_string()),
            timestamp: 0,
        }))
    }

    fn assistant_message(text: &str) -> crate::types::AgentMessage {
        crate::types::AgentMessage::Llm(Box::new(elph_ai::Message::Assistant(elph_ai::faux_assistant_message(
            vec![elph_ai::faux_text(text)],
            None,
        ))))
    }

    fn message_entry(id: &str, parent_id: Option<&str>, message: crate::types::AgentMessage) -> SessionTreeEntry {
        SessionTreeEntry::Message {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            prompt_title: String::new(),
            prompt_kind: String::new(),
            message,
        }
    }

    #[test]
    fn find_cut_point_keeps_recent_tokens() {
        let entries = vec![
            message_entry("u1", None, user_message(&"a".repeat(400))),
            message_entry("a1", Some("u1"), assistant_message("short")),
            message_entry("u2", Some("a1"), user_message(&"b".repeat(400))),
            message_entry("a2", Some("u2"), assistant_message("tail")),
        ];
        let cut = find_cut_point(&entries, 0, entries.len(), 50);
        assert!(cut.first_kept_entry_index >= 2);
    }

    #[test]
    fn find_turn_start_index_finds_user_turn() {
        let entries = vec![
            message_entry("u1", None, user_message("start")),
            message_entry("a1", Some("u1"), assistant_message("middle")),
            message_entry("a2", Some("a1"), assistant_message("end")),
        ];
        assert_eq!(find_turn_start_index(&entries, 2, 0), Some(0));
    }
}
