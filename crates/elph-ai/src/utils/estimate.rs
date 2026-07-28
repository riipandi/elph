//! Context token estimation with optional reuse of last assistant usage.
//!
//! The core token-counting function [`count_tokens_text`] is a port of
//! [`tokenx`](https://github.com/johannschopplich/tokenx): a heuristic
//! estimator calibrated against OpenAI's `o200k_base` encoding with ~96 %
//! accuracy and zero dependencies (std only).

use crate::types::{AssistantContentBlock, ContentBlock, Context, Message, StopReason, Tool, UserContent};

// ---------------------------------------------------------------------------
// Tokenx algorithm — grouped per segment
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum SegKind {
    Whitespace,
    Punctuation,
    Other,
}

fn seg_kind(c: char) -> SegKind {
    if c.is_whitespace() {
        SegKind::Whitespace
    } else if matches!(
        c,
        '.' | ','
            | '!'
            | '?'
            | ';'
            | ':'
            | '('
            | ')'
            | '{'
            | '}'
            | '['
            | ']'
            | '<'
            | '>'
            | '/'
            | '\\'
            | '|'
            | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '+'
            | '='
            | '`'
            | '~'
            | '"'
            | '_'
            | '-'
    ) {
        SegKind::Punctuation
    } else {
        SegKind::Other
    }
}

/// Fast token count estimation — tokenx algorithm port.
///
/// Splits text by whitespace/punctuation boundaries and applies heuristics
/// per segment with multi-language support.
pub fn count_tokens_text(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }

    let mut total = 0u64;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let kind = seg_kind(chars[i]);
        let mut j = i + 1;
        while j < chars.len() && seg_kind(chars[j]) == kind {
            j += 1;
        }

        total += match kind {
            SegKind::Whitespace => {
                // Structural whitespace (indentation/blank lines) costs 1 token.
                if chars[i..j].contains(&'\n') { 1 } else { 0 }
            }
            SegKind::Punctuation => {
                let len = j - i;
                match len {
                    0 => 0,
                    1..=3 => 1,
                    _ => (len as u64).div_ceil(2),
                }
            }
            SegKind::Other => {
                let segment: String = chars[i..j].iter().collect();
                estimate_other_segment(&segment)
            }
        };

        i = j;
    }

    total
}

// ---- Language helpers ----------------------------------------------------

fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        let cp = c as u32;
        matches!(cp,
            // CJK Unified Ideographs & Extensions
            0x4E00..=0x9FFF | 0x3400..=0x4DBF |
            // CJK Symbols, Hiragana, Katakana
            0x3000..=0x30FF |
            // Fullwidth forms
            0xFF00..=0xFFEF |
            // CJK Radicals, Strokes, Compatibility
            0x2E80..=0x2EFF | 0x31C0..=0x31EF |
            0x3200..=0x32FF | 0x3300..=0x33FF |
            // Hangul (Korean)
            0xAC00..=0xD7AF | 0x1100..=0x11FF |
            0x3130..=0x318F | 0xA960..=0xA97F | 0xD7B0..=0xD7FF
        )
    })
}

fn is_kana(c: char) -> bool {
    let cp = c as u32;
    (0x3040..=0x30FF).contains(&cp) // Hiragana + Katakana
}

fn estimate_cjk_segment(segment: &str) -> u64 {
    let (kana, other) = segment
        .chars()
        .fold((0u64, 0u64), |(k, o), c| if is_kana(c) { (k + 1, o) } else { (k, o + 1) });
    // Kana: 1.35 chars/token (multi-character particles/words).
    // Other CJK: 1 char ≈ 1 token (conservative upper bound).
    other + ((kana as f64) / 1.35).ceil() as u64
}

fn is_emoji_char(c: char) -> bool {
    let cp = c as u32;
    // Major emoji ranges (covers the vast majority of everyday emoji).
    (0x2600..=0x27BF).contains(&cp)   // Misc Symbols, Dingbats
        || (0x1F300..=0x1F9FF).contains(&cp) // Misc Symbols, Emoticons, SMP
        || (0x1FA00..=0x1FAFF).contains(&cp) // Chess, Symbols Extended-A
        || cp == 0x200D // Zero Width Joiner (ZWJ sequences)
        || cp == 0xFE0F // Variation Selector-16 (emoji style)
        || (0x231A..=0x23FA).contains(&cp) // Misc Technical (watch, clocks…)
}

/// Check if the segment consists entirely of emoji characters.
fn is_pure_emoji(segment: &str) -> bool {
    let mut non_empty = false;
    for c in segment.chars() {
        if !is_emoji_char(c) {
            return false;
        }
        non_empty = true;
    }
    non_empty
}

// Language-specific pattern checks (order matches tokenx — first match wins).
fn detect_language_ratio(segment: &str) -> Option<f64> {
    // 1. German umlauts
    if segment.chars().any(|c| matches!(c, 'ä' | 'ö' | 'ü' | 'ß' | 'ẞ')) {
        return Some(2.6);
    }
    // 2. French / Spanish accented
    if segment.chars().any(|c| {
        matches!(
            c,
            'é' | 'è'
                | 'ê'
                | 'ë'
                | 'à'
                | 'â'
                | 'î'
                | 'ï'
                | 'ô'
                | 'û'
                | 'ù'
                | 'ÿ'
                | 'ç'
                | 'œ'
                | 'æ'
                | 'á'
                | 'í'
                | 'ó'
                | 'ú'
                | 'ñ'
        )
    }) {
        return Some(3.0);
    }
    // 3. Slavic accented
    if segment.chars().any(|c| {
        matches!(
            c,
            'ą' | 'ć'
                | 'ę'
                | 'ł'
                | 'ń'
                | 'ś'
                | 'ź'
                | 'ż'
                | 'ě'
                | 'š'
                | 'č'
                | 'ř'
                | 'ž'
                | 'ý'
                | 'ů'
                | 'ď'
                | 'ť'
                | 'ň'
        )
    }) {
        return Some(2.5);
    }
    // 4. Cyrillic
    if segment.chars().any(|c| {
        let cp = c as u32;
        (0x0430..=0x044F).contains(&cp) || cp == 0x0451
    }) {
        return Some(4.0);
    }
    // 5. Greek accented
    if segment.chars().any(|c| {
        let cp = c as u32;
        (0x03AC..=0x03CE).contains(&cp)
    }) {
        return Some(2.75);
    }
    None
}

/// Count tokens for an "Other"-type segment (word, CJK, number, …).
fn estimate_other_segment(segment: &str) -> u64 {
    let char_count = segment.chars().count() as u64;
    if char_count == 0 {
        return 0;
    }

    // 1. Pure emoji → 0.75 chars/token (~1.33 tokens per emoji).
    if is_pure_emoji(segment) {
        return ((char_count as f64) / 0.75).ceil() as u64;
    }

    // 2. Language-specific heuristics (accented Latin, Cyrillic, Greek).
    if let Some(ratio) = detect_language_ratio(segment) {
        return ((char_count as f64) / ratio).ceil() as u64;
    }

    // 3. CJK — separate Kana from other CJK characters.
    if has_cjk(segment) {
        return estimate_cjk_segment(segment);
    }

    // 4. Pure numeric → ceil(digits / 3).
    if segment.bytes().all(|b| b.is_ascii_digit()) {
        return char_count.div_ceil(3);
    }

    // 5. Very short words (≤3 chars) cost at least 1 token.
    if char_count <= 3 {
        return 1;
    }

    // 6. Default: ~6 chars per token (calibrated to o200k_base).
    char_count.div_ceil(6)
}

// ---------------------------------------------------------------------------
// Pre-existing context-token-estimation API (unchanged except for the
// inner heuristic which now delegates to `count_tokens_text`).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextTokenEstimate {
    pub tokens: u32,
    pub usage_tokens: u32,
    pub trailing_tokens: u32,
    pub last_usage_index: Option<usize>,
}

fn estimate_text_tokens(text: &str) -> u32 {
    count_tokens_text(text) as u32
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
