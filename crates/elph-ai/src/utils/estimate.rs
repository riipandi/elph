//! Context token estimation with optional reuse of last assistant usage.

use crate::types::{AssistantContentBlock, ContentBlock, Context, Message, StopReason, Tool, UserContent};

const CHARS_PER_TOKEN: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextTokenEstimate {
    pub tokens: u32,
    pub usage_tokens: u32,
    pub trailing_tokens: u32,
    pub last_usage_index: Option<usize>,
}

fn estimate_text_tokens(text: &str) -> u32 {
    (text.chars().count() as f64 / CHARS_PER_TOKEN).ceil() as u32
}

fn estimate_message_tokens(message: &Message) -> u32 {
    match message {
        Message::User { content, .. } => match content {
            UserContent::Text(t) => estimate_text_tokens(t),
            UserContent::Blocks(blocks) => blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => estimate_text_tokens(text),
                    ContentBlock::Image { .. } => 1000,
                })
                .sum(),
        },
        Message::Assistant(m) => m
            .content
            .iter()
            .map(|block| match block {
                AssistantContentBlock::Text(t) => estimate_text_tokens(&t.text),
                AssistantContentBlock::Thinking(t) => estimate_text_tokens(&t.thinking),
                AssistantContentBlock::ToolCall(tc) => {
                    estimate_text_tokens(&tc.name)
                        + estimate_text_tokens(&tc.id)
                        + estimate_text_tokens(&tc.arguments.to_string())
                }
            })
            .sum(),
        Message::ToolResult { content, .. } => content
            .iter()
            .map(|b| match b {
                ContentBlock::Text { text } => estimate_text_tokens(text),
                ContentBlock::Image { .. } => 1000,
            })
            .sum(),
    }
}

fn calculate_context_tokens_from_usage(usage: &crate::types::Usage) -> u32 {
    if usage.total_tokens > 0 {
        usage.total_tokens as u32
    } else {
        (usage.input + usage.output + usage.cache_read + usage.cache_write) as u32
    }
}

/// Find the last assistant usage that still describes the current message prefix.
///
/// A newer prefix message (for example a compaction summary) inserted after an
/// assistant response invalidates that usage for the current prefix (#6464).
fn get_last_assistant_usage_info(messages: &[Message]) -> Option<(crate::types::Usage, usize)> {
    let mut latest_prefix_timestamp = i64::MIN;
    let mut usage_info: Option<(crate::types::Usage, usize)> = None;

    for (i, message) in messages.iter().enumerate() {
        if let Message::Assistant(assistant) = message {
            let usage_applies_to_prefix = assistant.timestamp >= latest_prefix_timestamp;
            let tokens = calculate_context_tokens_from_usage(&assistant.usage);
            if usage_applies_to_prefix
                && !matches!(assistant.stop_reason, StopReason::Aborted | StopReason::Error)
                && tokens > 0
            {
                usage_info = Some((assistant.usage.clone(), i));
            }
        }
        let ts = match message {
            Message::User { timestamp, .. }
            | Message::ToolResult { timestamp, .. }
            | Message::Assistant(crate::types::AssistantMessage { timestamp, .. }) => *timestamp,
        };
        latest_prefix_timestamp = latest_prefix_timestamp.max(ts);
    }

    usage_info
}

fn estimate_tools_tokens(tools: Option<&[Tool]>) -> u32 {
    let Some(tools) = tools else {
        return 0;
    };
    if tools.is_empty() {
        return 0;
    }
    let json = serde_json::to_string(tools).unwrap_or_default();
    estimate_text_tokens(&json)
}

fn estimate_messages(messages: &[Message]) -> ContextTokenEstimate {
    if let Some((usage, index)) = get_last_assistant_usage_info(messages) {
        let usage_tokens = calculate_context_tokens_from_usage(&usage);
        let trailing_tokens: u32 = messages[index + 1..].iter().map(estimate_message_tokens).sum();
        return ContextTokenEstimate {
            tokens: usage_tokens + trailing_tokens,
            usage_tokens,
            trailing_tokens,
            last_usage_index: Some(index),
        };
    }

    let tokens: u32 = messages.iter().map(estimate_message_tokens).sum();
    ContextTokenEstimate {
        tokens,
        usage_tokens: 0,
        trailing_tokens: tokens,
        last_usage_index: None,
    }
}

pub fn estimate_context_tokens(context: &Context) -> ContextTokenEstimate {
    let mut estimate = estimate_messages(&context.messages);
    if let Some(last_idx) = estimate.last_usage_index {
        let mut added_names = std::collections::HashSet::new();
        for message in &context.messages[last_idx + 1..] {
            if let Message::ToolResult {
                added_tool_names: Some(names),
                ..
            } = message
            {
                for name in names {
                    added_names.insert(name.as_str());
                }
            }
        }
        if !added_names.is_empty()
            && let Some(tools) = &context.tools
        {
            let added: Vec<Tool> = tools
                .iter()
                .filter(|t| added_names.contains(t.name.as_str()))
                .cloned()
                .collect();
            let added_tool_tokens = estimate_tools_tokens(Some(&added));
            estimate.tokens += added_tool_tokens;
            estimate.trailing_tokens += added_tool_tokens;
        }
    } else if let Some(sp) = &context.system_prompt {
        estimate.tokens += estimate_text_tokens(sp);
        estimate.trailing_tokens += estimate_text_tokens(sp);
        let tool_tokens = estimate_tools_tokens(context.tools.as_deref());
        estimate.tokens += tool_tokens;
        estimate.trailing_tokens += tool_tokens;
    } else {
        let tool_tokens = estimate_tools_tokens(context.tools.as_deref());
        estimate.tokens += tool_tokens;
        estimate.trailing_tokens += tool_tokens;
    }

    // Always count system prompt once when we reused usage (usage usually excludes system? — include conservatively)
    if estimate.last_usage_index.is_some()
        && let Some(sp) = &context.system_prompt
    {
        // Provider usage typically includes system in the last turn; do not double-count.
        let _ = sp;
    }

    estimate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AssistantMessage, Model, Usage};

    fn dummy_model() -> Model {
        Model {
            id: "m".into(),
            name: "m".into(),
            api: "openai-responses".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            reasoning: false,
            thinking_level_map: None,
            input: vec!["text".into()],
            cost: crate::types::ModelCost::default(),
            context_window: 128000,
            max_tokens: 4096,
            headers: None,
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        }
    }

    #[test]
    fn stale_usage_before_newer_prefix_is_ignored() {
        let model = dummy_model();
        let mut assistant = AssistantMessage::empty(&model);
        assistant.timestamp = 100;
        assistant.usage = Usage {
            total_tokens: 5000,
            ..Default::default()
        };
        // Compaction summary inserted at the head with a newer timestamp; older
        // assistant usage no longer describes the current prefix.
        let messages = vec![
            Message::User {
                content: UserContent::Text("compaction summary".into()),
                timestamp: 200,
            },
            Message::Assistant(assistant),
        ];
        let estimate = estimate_messages(&messages);
        assert_eq!(estimate.last_usage_index, None);
        assert_eq!(estimate.usage_tokens, 0);
    }
}
