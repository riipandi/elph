//! Short-lived transcript notices that upsert in place and expire automatically.

use std::time::{Duration, Instant};

use crate::tui::labels::agent_mode_change_notice;
use crate::types::AgentMode;

use super::types::{TranscriptMessage, TranscriptStyle};

pub const AGENT_MODE_NOTICE_KEY: &str = "transient:agent_mode";
pub const AGENT_MODE_NOTICE_TTL: Duration = Duration::from_secs(3);

/// Upsert a keyed notice; repeated calls replace the same row instead of stacking.
pub fn upsert_ephemeral_notice(
    messages: &mut Vec<TranscriptMessage>,
    key: &str,
    content: impl Into<String>,
    style: TranscriptStyle,
) {
    let content = content.into();
    if let Some(row) = messages
        .iter_mut()
        .find(|message| message.startup_key.as_deref() == Some(key))
    {
        row.content = content;
        row.style = style;
        return;
    }
    messages.push(TranscriptMessage::startup_status(key, content, style));
}

pub fn remove_ephemeral_notice(messages: &mut Vec<TranscriptMessage>, key: &str) -> bool {
    let before = messages.len();
    messages.retain(|message| message.startup_key.as_deref() != Some(key));
    messages.len() < before
}

pub fn show_agent_mode_notice(messages: &mut Vec<TranscriptMessage>, mode: AgentMode) {
    upsert_ephemeral_notice(
        messages,
        AGENT_MODE_NOTICE_KEY,
        agent_mode_change_notice(mode),
        TranscriptStyle::Meta,
    );
}

pub fn agent_mode_notice_expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|until| Instant::now() >= until)
}

pub fn next_agent_mode_notice_deadline() -> Instant {
    Instant::now() + AGENT_MODE_NOTICE_TTL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_replaces_existing_agent_mode_row() {
        let mut messages = Vec::new();
        show_agent_mode_notice(&mut messages, AgentMode::Plan);
        show_agent_mode_notice(&mut messages, AgentMode::Ask);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Agent mode: ask.");
        assert_eq!(messages[0].startup_key.as_deref(), Some(AGENT_MODE_NOTICE_KEY));
    }

    #[test]
    fn remove_ephemeral_notice_drops_keyed_row() {
        let mut messages = Vec::new();
        show_agent_mode_notice(&mut messages, AgentMode::Brave);
        assert!(remove_ephemeral_notice(&mut messages, AGENT_MODE_NOTICE_KEY));
        assert!(messages.is_empty());
    }

    #[test]
    fn agent_mode_notice_expired_after_deadline() {
        let deadline = Instant::now() - Duration::from_millis(1);
        assert!(agent_mode_notice_expired(Some(deadline)));
        assert!(!agent_mode_notice_expired(None));
    }
}
