//! Typed transcript entries for the Owly chat stream.

use elph_tui::{ToolExecutionState, TranscriptEntry};

/// Semantic kind for Owly transcript rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwlyEntryKind {
    Hint,
    User,
    Assistant,
    Thinking,
    Status,
    CommandHeader,
    CommandResult,
    ToolSummary,
}

/// One row in the Owly transcript with explicit layout kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwlyEntry {
    pub kind: OwlyEntryKind,
    pub inner: TranscriptEntry,
}

impl OwlyEntry {
    pub fn hint(content: impl Into<String>) -> Self {
        Self {
            kind: OwlyEntryKind::Hint,
            inner: TranscriptEntry::assistant(content.into()),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            kind: OwlyEntryKind::User,
            inner: TranscriptEntry::user(content),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            kind: OwlyEntryKind::Assistant,
            inner: TranscriptEntry::assistant(content),
        }
    }

    pub fn assistant_streaming(content: impl Into<String>) -> Self {
        Self {
            kind: OwlyEntryKind::Assistant,
            inner: TranscriptEntry::assistant_streaming(content),
        }
    }

    pub fn thinking(content: impl Into<String>) -> Self {
        Self {
            kind: OwlyEntryKind::Thinking,
            inner: TranscriptEntry::thinking(content, false),
        }
    }

    pub fn status(content: impl Into<String>) -> Self {
        Self {
            kind: OwlyEntryKind::Status,
            inner: TranscriptEntry::assistant(content),
        }
    }

    pub fn command_header(command: &str, provider: &str, model: &str) -> Self {
        Self {
            kind: OwlyEntryKind::CommandHeader,
            inner: TranscriptEntry::assistant(format!("{command} · {provider} · {model}")),
        }
    }

    pub fn tool_summary(tool: ToolExecutionState) -> Self {
        Self {
            kind: OwlyEntryKind::ToolSummary,
            inner: TranscriptEntry::tool(tool),
        }
    }
}

/// Build command result entry with success flag encoded for the renderer.
pub fn command_result_entry(message: &str, success: bool) -> OwlyEntry {
    let prefix = if success { "✓" } else { "✗" };
    OwlyEntry {
        kind: OwlyEntryKind::CommandResult,
        inner: TranscriptEntry::assistant(format!("{prefix} {message}")),
    }
}
