//! Rich transcript types for agent chat UI.

use std::time::{Duration, Instant};

/// Default maximum transcript entries retained in memory (oldest dropped first).
pub const DEFAULT_TRANSCRIPT_CAP: usize = 500;

/// Truncates `entries` to the newest `max` items. No-op when `max` is zero.
pub fn cap_entries<T>(entries: &mut Vec<T>, max: usize) {
    if max == 0 {
        return;
    }
    let excess = entries.len().saturating_sub(max);
    if excess > 0 {
        entries.drain(..excess);
    }
}

/// Appends `item` and drops the oldest entries when the list exceeds `max`.
pub fn push_capped<T>(entries: &mut Vec<T>, item: T, max: usize) {
    entries.push(item);
    cap_entries(entries, max);
}

/// Role of a transcript entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptRole {
    User,
    Assistant,
    Tool,
    Thinking,
    System,
}

/// Lifecycle state for a tool execution card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionStatus {
    Pending,
    Running,
    Success,
    Error,
    Cancelled,
}

/// Tool execution payload for UI cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionState {
    pub id: String,
    pub name: String,
    pub args_summary: String,
    pub status: ToolExecutionStatus,
    pub output: String,
    pub requires_approval: bool,
}

impl ToolExecutionState {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            args_summary: String::new(),
            status: ToolExecutionStatus::Pending,
            output: String::new(),
            requires_approval: false,
        }
    }

    pub fn with_args(mut self, summary: impl Into<String>) -> Self {
        self.args_summary = summary.into();
        self
    }

    pub fn with_status(mut self, status: ToolExecutionStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = output.into();
        self
    }
}

/// One message in the agent transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEntry {
    pub role: TranscriptRole,
    pub content: String,
    pub is_streaming: bool,
    pub tool: Option<ToolExecutionState>,
    pub thinking_expanded: bool,
    pub timestamp: Option<String>,
}

impl TranscriptEntry {
    pub fn user(content: impl Into<String>) -> Self {
        Self::user_with_timestamp(content, None)
    }

    pub fn user_with_timestamp(content: impl Into<String>, timestamp: Option<String>) -> Self {
        Self {
            role: TranscriptRole::User,
            content: content.into(),
            is_streaming: false,
            tool: None,
            thinking_expanded: false,
            timestamp,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::assistant_with_timestamp(content, None)
    }

    pub fn assistant_with_timestamp(content: impl Into<String>, timestamp: Option<String>) -> Self {
        Self {
            role: TranscriptRole::Assistant,
            content: content.into(),
            is_streaming: false,
            tool: None,
            thinking_expanded: false,
            timestamp,
        }
    }

    pub fn assistant_streaming(content: impl Into<String>) -> Self {
        Self {
            role: TranscriptRole::Assistant,
            content: content.into(),
            is_streaming: true,
            tool: None,
            thinking_expanded: false,
            timestamp: None,
        }
    }

    pub fn thinking(content: impl Into<String>, expanded: bool) -> Self {
        Self {
            role: TranscriptRole::Thinking,
            content: content.into(),
            is_streaming: false,
            tool: None,
            thinking_expanded: expanded,
            timestamp: None,
        }
    }

    pub fn tool(state: ToolExecutionState) -> Self {
        Self {
            role: TranscriptRole::Tool,
            content: String::new(),
            is_streaming: false,
            tool: Some(state),
            thinking_expanded: false,
            timestamp: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: TranscriptRole::System,
            content: content.into(),
            is_streaming: false,
            tool: None,
            thinking_expanded: false,
            timestamp: None,
        }
    }
}

/// Throttled buffer for incremental assistant streaming.
#[derive(Debug, Clone)]
pub struct StreamingBuffer {
    content: String,
    last_flush: Option<Instant>,
    min_interval: Duration,
}

impl Default for StreamingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingBuffer {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            last_flush: None,
            min_interval: Duration::from_millis(16),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.min_interval = interval;
        self
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    pub fn reset(&mut self) {
        self.content.clear();
        self.last_flush = None;
    }

    /// Appends a delta. Returns `true` when consumers should re-render.
    pub fn push(&mut self, delta: &str) -> bool {
        if delta.is_empty() {
            return false;
        }
        self.content.push_str(delta);
        let now = Instant::now();
        if self
            .last_flush
            .is_none_or(|t| now.duration_since(t) >= self.min_interval)
        {
            self.last_flush = Some(now);
            true
        } else {
            false
        }
    }

    /// Forces a flush on the next read.
    pub fn flush(&mut self) -> bool {
        if self.content.is_empty() {
            return false;
        }
        self.last_flush = Some(Instant::now());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_buffer_accumulates() {
        let mut buf = StreamingBuffer::new();
        buf.push("Hello");
        assert_eq!(buf.content(), "Hello");
    }

    #[test]
    fn transcript_entry_constructors() {
        let user = TranscriptEntry::user("hi");
        assert_eq!(user.role, TranscriptRole::User);
        let tool = TranscriptEntry::tool(ToolExecutionState::new("1", "bash"));
        assert_eq!(tool.role, TranscriptRole::Tool);
    }

    #[test]
    fn cap_entries_drops_oldest() {
        let mut entries: Vec<u32> = (0..10).collect();
        cap_entries(&mut entries, 4);
        assert_eq!(entries, vec![6, 7, 8, 9]);
    }

    #[test]
    fn push_capped_enforces_limit() {
        let mut entries = Vec::new();
        for i in 0..5 {
            push_capped(&mut entries, i, 3);
        }
        assert_eq!(entries, vec![2, 3, 4]);
    }
}
