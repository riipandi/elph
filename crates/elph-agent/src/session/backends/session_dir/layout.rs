//! Multi-file session directory layout.

/// Session summary metadata (`summary.json`).
pub const SUMMARY_FILE: &str = "summary.json";
/// Flat conversation log for display and export.
pub const CHAT_HISTORY_FILE: &str = "chat_history.jsonl";
/// Append-only session tree entries (authoritative branch state).
pub const EVENTS_FILE: &str = "events.jsonl";
/// UI / protocol session updates.
pub const UPDATES_FILE: &str = "updates.jsonl";
/// Prompt assembly context.
pub const PROMPT_CONTEXT_FILE: &str = "prompt_context.json";
/// Current system prompt text.
pub const SYSTEM_PROMPT_FILE: &str = "system_prompt.txt";
/// User prompt submission log.
pub const PROMPT_HISTORY_FILE: &str = "prompt_history.jsonl";

/// Subdirectories created for every new session.
pub const SESSION_SUBDIRS: &[&str] = &["terminals", "compaction_checkpoints", "compaction_requests"];

pub const CHAT_FORMAT_VERSION: u32 = 1;
pub const PROMPT_CONTEXT_VERSION: u32 = 1;
