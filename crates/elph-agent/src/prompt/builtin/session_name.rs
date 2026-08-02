//! Auto session naming prompts and text helpers.

/// Maximum characters for an auto-generated session title.
pub const SESSION_NAME_MAX_LEN: usize = 60;

/// Character budget for the naming excerpt (first + most recent user messages).
///
/// Keeps the naming LLM call cheap and focused even for very long sessions.
pub const SESSION_NAME_EXCERPT_MAX_CHARS: usize = 3_500;

/// Titles that carry no information about the conversation. Rejected so the
/// session keeps its placeholder instead of a useless label.
const GENERIC_SESSION_NAMES: &[&str] = &[
    "ai chat",
    "ai conversation",
    "chat",
    "chat conversation",
    "chat session",
    "coding session",
    "conversation",
    "conversation session",
    "developer session",
    "dev session",
    "general",
    "general chat",
    "hello",
    "help",
    "hey",
    "hi",
    "new chat",
    "new session",
    "session",
    "session chat",
    "test",
    "untitled",
];

/// System prompt for the naming LLM call.
pub const SESSION_NAME_SYSTEM_PROMPT: &str =
    "You produce short conversation titles. Output only the title text, nothing else.";

/// Build the user prompt for session title generation.
pub fn build_session_name_prompt(conversation: &str) -> String {
    format!(
        "You are naming a conversation session. Based on the conversation below, produce a single short title \
         (max {SESSION_NAME_MAX_LEN} characters, no quotes). Be specific — mention the main task, file, or topic. \
         Use sentence case.\n\n<conversation>\n{conversation}\n</conversation>"
    )
}

/// Normalize a raw model title into a display-safe session name.
///
/// Strips surrounding quotes, whitespace, and trailing punctuation; collapses
/// inner whitespace; drops a leading `Title:`/`Session:`/`Name:` label; rejects
/// generic placeholder titles (returns `""`); truncates to
/// [`SESSION_NAME_MAX_LEN`] characters.
pub fn sanitize_session_name(raw: &str) -> String {
    let stripped: String = raw
        .trim()
        .chars()
        .filter(|ch| !matches!(ch, '"' | '\'' | '“' | '”' | '‘' | '’'))
        .collect();
    let label_free = strip_label_prefix(&stripped);
    let oneline = label_free.split_whitespace().collect::<Vec<_>>().join(" ");
    let oneline = trim_trailing_punctuation(&oneline);
    if oneline.is_empty() || is_generic_session_name(&oneline) {
        return String::new();
    }
    if oneline.chars().count() > SESSION_NAME_MAX_LEN {
        oneline.chars().take(SESSION_NAME_MAX_LEN).collect()
    } else {
        oneline
    }
}

/// Drop a leading `Title:` / `Session:` / `Name:` label (any casing) that some
/// models prefix to their answer.
fn strip_label_prefix(raw: &str) -> &str {
    for label in ["Title:", "Session:", "Name:"] {
        if raw.len() >= label.len() && raw[..label.len()].eq_ignore_ascii_case(label) {
            return raw[label.len()..].trim_start();
        }
    }
    raw
}

/// Remove sentence-ending punctuation noise so titles read cleanly in lists.
fn trim_trailing_punctuation(s: &str) -> String {
    s.trim_end_matches(['.', ',', ';', ':', '-', '–', '—', '…', ' ', '\t'])
        .to_string()
}

/// Whether the title is a generic placeholder that tells nothing about the session.
fn is_generic_session_name(title: &str) -> bool {
    let normalized = title.trim().to_lowercase();
    GENERIC_SESSION_NAMES.contains(&normalized.as_str())
}

/// Extract user messages from the transcript for naming (tool results omitted).
///
/// For long conversations the excerpt keeps the **first** user message (initial
/// intent) plus the **most recent** messages that fit within
/// [`SESSION_NAME_EXCERPT_MAX_CHARS`], dropping the noisy middle.
pub fn extract_conversation_for_naming(messages: &[crate::types::AgentMessage]) -> String {
    use elph_ai::{ContentBlock, Message, UserContent};

    let mut parts = Vec::new();
    for message in messages {
        let Some(Message::User { content, .. }) = message.as_llm() else {
            continue;
        };
        let text = match content {
            UserContent::Text(value) => value.clone(),
            UserContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        };
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            parts.push(format!("User: {trimmed}"));
        }
    }

    let total_chars: usize = parts.iter().map(|part| part.chars().count()).sum();
    if parts.is_empty() || total_chars <= SESSION_NAME_EXCERPT_MAX_CHARS {
        return parts.join("\n\n");
    }

    // Large conversation: keep the first message and the most recent messages
    // that still fit the budget (minus the "[…]" separator).
    let first = parts[0].clone();
    let tail_budget = SESSION_NAME_EXCERPT_MAX_CHARS.saturating_sub(first.chars().count() + 5);
    let mut tail = Vec::new();
    let mut tail_chars = 0usize;
    for part in parts.iter().skip(1).rev() {
        let cost = part.chars().count() + 2; // "\n\n" separator
        if !tail.is_empty() && tail_chars + cost > tail_budget {
            break;
        }
        tail.push(part.clone());
        tail_chars += cost;
    }
    tail.reverse();

    let mut out = String::with_capacity(SESSION_NAME_EXCERPT_MAX_CHARS + 32);
    out.push_str(&first);
    out.push_str("\n\n[...]");
    for part in tail {
        out.push_str("\n\n");
        out.push_str(&part);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentMessage;
    use elph_ai::{Message, UserContent};

    #[test]
    fn sanitize_strips_quotes_and_truncates() {
        let long = "a".repeat(80);
        assert_eq!(sanitize_session_name(&format!("\"{long}\"")), "a".repeat(60));
        assert_eq!(sanitize_session_name("  Fix login bug  "), "Fix login bug");
    }

    #[test]
    fn sanitize_strips_labels_and_trailing_punctuation() {
        assert_eq!(sanitize_session_name("Title: Fix login bug."), "Fix login bug");
        assert_eq!(sanitize_session_name("session: Refactor auth flow…"), "Refactor auth flow");
        assert_eq!(sanitize_session_name("Name: Add tests —"), "Add tests");
        assert_eq!(sanitize_session_name("Refactor CI:"), "Refactor CI");
        assert_eq!(sanitize_session_name("  Debug  the  parser  "), "Debug the parser");
        // Questions keep their question mark.
        assert_eq!(sanitize_session_name("How to fix login?"), "How to fix login?");
    }

    #[test]
    fn sanitize_rejects_generic_titles() {
        for generic in [
            "Chat",
            "New chat",
            "Conversation",
            "Chat session",
            "General",
            "Hello",
            "hi",
            "Help",
            "Test",
        ] {
            assert_eq!(sanitize_session_name(generic), "", "generic title {generic:?} must be rejected");
        }
        assert_eq!(sanitize_session_name("Chat."), "");
        assert_eq!(sanitize_session_name("Title: Session"), "");
    }

    #[test]
    fn extract_conversation_collects_user_messages() {
        let messages = vec![
            AgentMessage::Llm(Box::new(Message::User {
                content: UserContent::Text("Explain auth flow".into()),
                timestamp: 0,
            })),
            AgentMessage::Llm(Box::new(Message::User {
                content: UserContent::Text("What about OAuth?".into()),
                timestamp: 0,
            })),
        ];
        let conversation = extract_conversation_for_naming(&messages);
        assert!(conversation.contains("User: Explain auth flow"));
        assert!(conversation.contains("User: What about OAuth?"));
    }

    #[test]
    fn extract_conversation_omits_tool_results() {
        let messages = vec![
            AgentMessage::Llm(Box::new(Message::User {
                content: UserContent::Text("Refactor login".into()),
                timestamp: 0,
            })),
            AgentMessage::Llm(Box::new(Message::ToolResult {
                tool_call_id: "call-1".into(),
                tool_name: "read_file".into(),
                content: vec![],
                details: None,
                added_tool_names: None,
                usage: None,
                is_error: false,
                timestamp: 0,
            })),
        ];
        let conversation = extract_conversation_for_naming(&messages);
        assert_eq!(conversation, "User: Refactor login");
    }

    #[test]
    fn extract_conversation_samples_long_sessions() {
        let mut messages = Vec::new();
        for i in 0..200 {
            messages.push(AgentMessage::Llm(Box::new(Message::User {
                content: UserContent::Text(format!("Message number {i} with a bit of filler text to inflate size")),
                timestamp: 0,
            })));
        }
        let conversation = extract_conversation_for_naming(&messages);
        assert!(conversation.contains("User: Message number 0"), "first message kept");
        assert!(conversation.contains("User: Message number 199"), "most recent message kept");
        assert!(conversation.contains("[...]"), "middle dropped with marker");
        assert!(conversation.chars().count() <= SESSION_NAME_EXCERPT_MAX_CHARS + 64);
        assert!(!conversation.contains("Message number 100"), "noisy middle dropped");
    }
}
