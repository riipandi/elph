//! Persist and restore the TUI transcript so session resume matches live state.
//!
//! Snapshots are stored as session-tree `Custom` entries (`elph.transcript.snapshot`).
//! Each completed agent turn replaces the logical snapshot by appending a new entry;
//! load always takes the **latest** snapshot. LLM branch reconstruction is a fallback
//! when no snapshot exists (e.g. mid-turn crash).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::markdown::AssistantMarkdownBuffer;
use super::markdown::parse_markdown_on_worker;
use super::types::{ToolCardDetail, TranscriptMessage, TranscriptStyle};

/// Session custom-entry type for full transcript snapshots.
pub const TRANSCRIPT_SNAPSHOT_CUSTOM_TYPE: &str = "elph.transcript.snapshot";

/// Nested tool-result details key for elph TUI metadata (duration, …).
pub const ELPH_UI_DETAILS_KEY: &str = "_elph_ui";

/// Serializable transcript row (markdown rehydrated on load).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchivedTranscriptMessage {
    pub content: String,
    pub style: TranscriptStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolCardDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub local_slash_response: bool,
    #[serde(default)]
    pub detail_expanded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_detail: Option<String>,
    #[serde(default)]
    pub status_indent: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSnapshot {
    /// Schema version for future migrations (project is new; still versioned).
    #[serde(default = "snapshot_version_default")]
    pub version: u32,
    pub messages: Vec<ArchivedTranscriptMessage>,
}

fn snapshot_version_default() -> u32 {
    1
}

impl From<&TranscriptMessage> for ArchivedTranscriptMessage {
    fn from(message: &TranscriptMessage) -> Self {
        Self {
            content: message.content.clone(),
            style: message.style,
            tool: message.tool.clone(),
            duration_secs: message.duration_secs,
            submitted_at: message.submitted_at,
            local_slash_response: message.local_slash_response,
            detail_expanded: message.detail_expanded,
            status_detail: message.status_detail.clone(),
            status_indent: message.status_indent,
        }
    }
}

impl ArchivedTranscriptMessage {
    /// Convert archive row back into a live transcript message (rebuild markdown if needed).
    pub fn into_transcript_message(self) -> TranscriptMessage {
        let mut message = TranscriptMessage {
            content: self.content,
            style: self.style,
            tool: self.tool,
            markdown: None,
            duration_secs: self.duration_secs,
            submitted_at: self.submitted_at,
            local_slash_response: self.local_slash_response,
            startup_key: None,
            detail_expanded: self.detail_expanded,
            status_detail: self.status_detail,
            status_indent: self.status_indent,
        };

        if message.style == TranscriptStyle::Assistant {
            let mut md = AssistantMarkdownBuffer::new();
            md.mark_stream_complete();
            md.refresh_stable(&message.content, 100);
            if let Some(part) = md.parts.first() {
                let hash = part.source_hash;
                let document = parse_markdown_on_worker(&message.content);
                md.apply_document(hash, document);
            }
            message.markdown = Some(md);
        }

        message
    }
}

/// Whether a live transcript row should be persisted across resume.
pub fn should_archive_message(message: &TranscriptMessage) -> bool {
    if message.startup_key.is_some() {
        return false;
    }
    if message.is_ephemeral_notice() || message.is_quit_busy_notice() {
        return false;
    }
    matches!(
        message.style,
        TranscriptStyle::User
            | TranscriptStyle::SkillPrompt
            | TranscriptStyle::Thinking
            | TranscriptStyle::Assistant
            | TranscriptStyle::ToolRunning
            | TranscriptStyle::ToolSuccess
            | TranscriptStyle::ToolFailed
            | TranscriptStyle::Error
    )
}

/// Build a snapshot value suitable for `append_custom_entry`.
pub fn build_snapshot_data(messages: &[TranscriptMessage]) -> serde_json::Value {
    let archived: Vec<ArchivedTranscriptMessage> = messages
        .iter()
        .filter(|m| should_archive_message(m))
        .map(ArchivedTranscriptMessage::from)
        .collect();
    let snapshot = TranscriptSnapshot {
        version: 1,
        messages: archived,
    };
    serde_json::to_value(snapshot).unwrap_or_else(|_| json!({ "version": 1, "messages": [] }))
}

/// Parse snapshot JSON into transcript messages.
pub fn messages_from_snapshot_data(data: &serde_json::Value) -> Option<Vec<TranscriptMessage>> {
    let snapshot: TranscriptSnapshot = serde_json::from_value(data.clone()).ok()?;
    Some(
        snapshot
            .messages
            .into_iter()
            .map(ArchivedTranscriptMessage::into_transcript_message)
            .collect(),
    )
}

/// Read `duration_secs` from tool-result details (`_elph_ui.duration_secs`).
pub fn duration_from_tool_details(details: &serde_json::Value) -> Option<f64> {
    details
        .get(ELPH_UI_DETAILS_KEY)
        .and_then(|ui| ui.get("duration_secs"))
        .and_then(|v| v.as_f64())
        .filter(|s| s.is_finite() && *s >= 0.0)
}

/// Merge wall duration into tool-result details under `_elph_ui`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn merge_duration_into_details(details: &mut serde_json::Value, duration_secs: f64) {
    if !duration_secs.is_finite() || duration_secs < 0.0 {
        return;
    }
    if !details.is_object() {
        *details = json!({});
    }
    let Some(obj) = details.as_object_mut() else {
        return;
    };
    let ui = obj.entry(ELPH_UI_DETAILS_KEY.to_string()).or_insert_with(|| json!({}));
    if let Some(ui_obj) = ui.as_object_mut() {
        ui_obj.insert("duration_secs".into(), json!(duration_secs));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trip_preserves_tool_diff_and_duration() {
        let mut msg = TranscriptMessage::tool_call("edit_file", r#"{"path":"a.rs"}"#, TranscriptStyle::ToolSuccess);
        {
            let tool = msg.tool.as_mut().unwrap();
            tool.output = "Edited a.rs".into();
            tool.old_text = Some("old\n".into());
            tool.new_text = Some("new\n".into());
            tool.file_path = Some("/tmp/a.rs".into());
        }
        msg.duration_secs = Some(1.25);
        msg.detail_expanded = true;

        let data = build_snapshot_data(&[msg]);
        let restored = messages_from_snapshot_data(&data).expect("parse");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].style, TranscriptStyle::ToolSuccess);
        assert_eq!(restored[0].duration_secs, Some(1.25));
        assert!(restored[0].detail_expanded);
        let tool = restored[0].tool.as_ref().unwrap();
        assert_eq!(tool.old_text.as_deref(), Some("old\n"));
        assert_eq!(tool.new_text.as_deref(), Some("new\n"));
        assert!(tool.has_inline_diff());
    }

    #[test]
    fn should_archive_skips_startup_and_meta() {
        let user = TranscriptMessage::text("hi", TranscriptStyle::User);
        assert!(should_archive_message(&user));
        let startup = TranscriptMessage::startup_status("startup:phase", "Loading", TranscriptStyle::Meta);
        assert!(!should_archive_message(&startup));
    }

    #[test]
    fn merge_and_read_duration_in_details() {
        let mut details = json!({ "old_content": "a", "new_content": "b" });
        merge_duration_into_details(&mut details, 2.5);
        assert_eq!(duration_from_tool_details(&details), Some(2.5));
        assert_eq!(details.get("old_content").and_then(|v| v.as_str()), Some("a"));
    }
}
