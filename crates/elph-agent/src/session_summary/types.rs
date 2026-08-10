//! Session summary domain types.

use serde::{Deserialize, Serialize};

/// A compaction summary stored for cross-session reference.
///
/// One row per session, upserted whenever a compaction completes (manual
/// `/compact` or auto-compaction). Other sessions can read this to recall
/// past context without replaying full history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummary {
    /// Session ID this summary belongs to (primary key).
    pub session_id: String,
    /// The latest compaction summary text.
    pub summary: String,
    /// Token count before the last compaction ran.
    pub tokens_before: i64,
    /// How many compactions have run for this session.
    pub compaction_count: i64,
    /// Entry ID of the first kept entry after the last compaction.
    pub first_kept_entry_id: Option<String>,
    /// JSON-encoded details (read/modified file lists, etc.).
    pub details: Option<String>,
    /// ISO timestamp of first summary creation.
    pub created_at: String,
    /// ISO timestamp of last summary update.
    pub updated_at: String,
}
