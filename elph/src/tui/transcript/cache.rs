//! Disk-backed transcript cache using Turso (local SQLite).
//!
//! # Architecture
//!
//! Hybrid sliding window: the N most recent messages live in memory (`active` Vec),
//! while older messages are archived to a local SQLite database. Layout metadata
//! (`start_rows`, `row_counts`) is always kept in memory so scroll metrics stay
//! cheap and stable.
//!
//! When the user scrolls back into archived territory, the panel triggers an async
//! load that populates the active window from disk.

use std::path::Path;

use anyhow::Result;
use turso::{Builder, Connection, params};

use super::types::{TranscriptMessage, TranscriptStyle};

/// Maximum number of messages kept in the in-memory sliding window.
const MAX_ACTIVE: usize = 200;
/// How many messages to drain from the front when the window overflows.
const ARCHIVE_BATCH: usize = 100;
/// How many pending archive rows to buffer before flushing to SQLite.
const FLUSH_BATCH: usize = 50;

// ---------------------------------------------------------------------------
// TranscriptCache — low-level SQLite operations
// ---------------------------------------------------------------------------

/// Low-level SQLite transcript store.
///
/// One database file per project, partitioned by `session_id`.
pub struct TranscriptCache {
    conn: Connection,
    session_id: String,
}

impl TranscriptCache {
    /// Open (or create) the transcript database and run migrations.
    pub async fn open(db_path: &Path, session_id: &str) -> Result<Self> {
        let db = Builder::new_local(db_path.to_string_lossy().as_ref())
            .experimental_multiprocess_wal(true)
            .build()
            .await?;
        let conn = db.connect()?;
        Self::run_migrations(&conn).await?;
        Ok(Self {
            conn,
            session_id: session_id.to_string(),
        })
    }

    /// Run schema migrations (idempotent).
    async fn run_migrations(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS transcript_messages (
                session_id  TEXT NOT NULL,
                seq         INTEGER NOT NULL,
                style       TEXT NOT NULL,
                content     TEXT NOT NULL,
                tool_name   TEXT,
                tool_args   TEXT,
                tool_output TEXT,
                tool_old    TEXT,
                tool_new    TEXT,
                tool_path   TEXT,
                duration    REAL,
                expanded    INTEGER NOT NULL DEFAULT 1,
                pinned      INTEGER NOT NULL DEFAULT 0,
                status      TEXT,
                indent      INTEGER NOT NULL DEFAULT 0,
                tree        TEXT,
                model       TEXT,
                agent       TEXT,
                user_shell  INTEGER NOT NULL DEFAULT 0,
                slash_resp  INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(session_id, seq)
            );
            CREATE INDEX IF NOT EXISTS idx_transcript_msg_session_seq
                ON transcript_messages(session_id, seq);",
        )
        .await?;
        Ok(())
    }

    /// Insert a batch of archived messages (individual inserts within a transaction).
    pub async fn push_batch(&self, batch: impl IntoIterator<Item = (usize, &TranscriptMessage)>) -> Result<()> {
        let sql = "INSERT OR IGNORE INTO transcript_messages \
                    (session_id, seq, style, content, \
                     tool_name, tool_args, tool_output, tool_old, tool_new, tool_path, \
                     duration, expanded, pinned, status, indent, tree, model, agent, \
                     user_shell, slash_resp) \
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, \
                            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)";

        for (seq, msg) in batch {
            self.conn
                .execute(
                    sql,
                    params![
                        self.session_id.as_str(),
                        seq as i64,
                        style_to_str(msg.style),
                        msg.content.as_str(),
                        msg.tool.as_ref().map(|t| t.name.as_str()),
                        msg.tool.as_ref().map(|t| t.args_summary.as_str()),
                        msg.tool.as_ref().map(|t| t.output.as_str()),
                        msg.tool.as_ref().and_then(|t| t.old_text.as_deref()),
                        msg.tool.as_ref().and_then(|t| t.new_text.as_deref()),
                        msg.tool.as_ref().and_then(|t| t.file_path.as_deref()),
                        msg.duration_secs,
                        msg.detail_expanded as i64,
                        msg.user_pinned as i64,
                        msg.status_detail.as_deref(),
                        msg.status_indent as i64,
                        msg.tree_prefix.as_deref(),
                        msg.model_tag.as_deref(),
                        msg.agent_tag.as_deref(),
                        msg.user_shell as i64,
                        msg.local_slash_response as i64,
                    ],
                )
                .await?;
        }
        Ok(())
    }

    /// Load a range of messages from disk by seq.
    pub async fn load_range(&self, start_seq: usize, end_seq: usize) -> Result<Vec<TranscriptMessage>> {
        let sql = "SELECT seq, style, content, \
                    tool_name, tool_args, tool_output, tool_old, tool_new, tool_path, \
                    duration, expanded, pinned, status, indent, tree, model, agent, \
                    user_shell, slash_resp \
                    FROM transcript_messages \
                    WHERE session_id = ?1 AND seq >= ?2 AND seq < ?3 \
                    ORDER BY seq ASC";
        let mut rows = self
            .conn
            .query(sql, params![self.session_id.as_str(), start_seq as i64, end_seq as i64])
            .await?;
        let mut out = Vec::with_capacity(end_seq.saturating_sub(start_seq));
        while let Some(row) = rows.next().await? {
            out.push(message_from_row(&row)?);
        }
        Ok(out)
    }

    /// Total number of archived messages for this session.
    pub async fn archived_count(&self) -> Result<usize> {
        let sql = "SELECT COUNT(*) FROM transcript_messages WHERE session_id = ?1";
        let mut rows = self.conn.query(sql, params![self.session_id.as_str()]).await?;
        if let Some(row) = rows.next().await? {
            Ok(row.get::<i64>(0)? as usize)
        } else {
            Ok(0)
        }
    }

    /// Delete all data for this session.
    pub async fn clear_session(&self) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM transcript_messages WHERE session_id = ?1",
                params![self.session_id.as_str()],
            )
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// CachedTranscript — hybrid in-memory + disk-backed message store
// ---------------------------------------------------------------------------

/// Hybrid transcript message store.
///
/// - `active[0]` corresponds to seq `base_seq`
/// - Messages with seq < `base_seq` are on disk
pub struct CachedTranscript {
    /// In-memory sliding window (most recent messages).
    active: Vec<TranscriptMessage>,
    /// Global seq offset of `active[0]`.
    base_seq: usize,
    /// Total number of messages ever pushed (active + archived).
    total: usize,
    /// Disk-backed cache (optional — present when a session_id is available).
    disk: Option<TranscriptCache>,
    /// Pending archive rows to be flushed to disk.
    pending_archive: Vec<(usize, TranscriptMessage)>,
    /// Whether archival is suppressed (e.g. during bootstrap).
    archival_suppressed: bool,
}

impl CachedTranscript {
    /// Create a new empty transcript cache.
    pub fn new(disk: Option<TranscriptCache>) -> Self {
        Self {
            active: Vec::with_capacity(MAX_ACTIVE),
            base_seq: 0,
            total: 0,
            disk,
            pending_archive: Vec::new(),
            archival_suppressed: false,
        }
    }

    /// Create a new cache pre-populated with startup messages.
    pub fn with_startup(disk: Option<TranscriptCache>, startup: Vec<TranscriptMessage>) -> Self {
        let total = startup.len();
        Self {
            active: startup,
            base_seq: 0,
            total,
            disk,
            pending_archive: Vec::new(),
            archival_suppressed: false,
        }
    }

    /// Suppress archival (used during bootstrap / startup).
    pub fn suppress_archival(&mut self) {
        self.archival_suppressed = true;
    }

    /// Resume archival (after bootstrap completes).
    pub fn resume_archival(&mut self) {
        self.archival_suppressed = false;
        self.maybe_archive();
    }

    /// Push a new message to the active window.
    ///
    /// If the window exceeds `MAX_ACTIVE`, the oldest `ARCHIVE_BATCH` messages
    /// are moved to the pending archive buffer.
    pub fn push(&mut self, msg: TranscriptMessage) {
        self.active.push(msg);
        self.total += 1;
        self.maybe_archive();
    }

    /// Get a message by global index.
    pub fn get(&self, index: usize) -> Option<&TranscriptMessage> {
        if index < self.base_seq {
            None
        } else {
            self.active.get(index - self.base_seq)
        }
    }

    /// Get a mutable reference to a message by global index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut TranscriptMessage> {
        if index < self.base_seq {
            None
        } else {
            self.active.get_mut(index - self.base_seq)
        }
    }

    /// Get the last message (for streaming append).
    pub fn last_mut(&mut self) -> Option<&mut TranscriptMessage> {
        self.active.last_mut()
    }

    /// Total number of messages (active + archived).
    pub fn len(&self) -> usize {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Number of messages currently in the active window.
    pub fn active_len(&self) -> usize {
        self.active.len()
    }

    /// Global seq of the first active message.
    pub fn base_seq(&self) -> usize {
        self.base_seq
    }

    /// Iterate over the active window.
    pub fn iter(&self) -> impl Iterator<Item = &TranscriptMessage> {
        self.active.iter()
    }

    /// Iterate over the active window with global indices.
    pub fn iter_enumerated(&self) -> impl Iterator<Item = (usize, &TranscriptMessage)> {
        let base = self.base_seq;
        self.active.iter().enumerate().map(move |(i, m)| (base + i, m))
    }

    /// Whether the given global index is archived (disk-only).
    pub fn is_archived(&self, index: usize) -> bool {
        index < self.base_seq
    }

    /// Whether the given global index is within the active window.
    pub fn is_active(&self, index: usize) -> bool {
        index >= self.base_seq && index < self.base_seq + self.active.len()
    }

    /// Check whether archival should happen and do it.
    fn maybe_archive(&mut self) {
        if self.archival_suppressed {
            return;
        }
        while self.active.len() > MAX_ACTIVE {
            let drained: Vec<TranscriptMessage> = self.active.drain(..ARCHIVE_BATCH).collect();
            for (i, msg) in drained.into_iter().enumerate() {
                self.pending_archive.push((self.base_seq + i, msg));
            }
            self.base_seq += ARCHIVE_BATCH;
        }
    }

    /// Flush pending archive rows to disk.
    pub async fn flush(&mut self) -> Result<()> {
        if self.pending_archive.is_empty() {
            return Ok(());
        }
        if let Some(ref disk) = self.disk {
            let batch: Vec<(usize, &TranscriptMessage)> =
                self.pending_archive.iter().map(|(seq, msg)| (*seq, msg)).collect();
            for chunk in batch.chunks(FLUSH_BATCH) {
                disk.push_batch(chunk.iter().copied()).await?;
            }
        }
        self.pending_archive.clear();
        Ok(())
    }

    /// Load archived messages from disk into the active window.
    pub async fn load_range(&self, start_seq: usize, end_seq: usize) -> Result<Vec<TranscriptMessage>> {
        let Some(ref disk) = self.disk else {
            return Ok(Vec::new());
        };
        disk.load_range(start_seq, end_seq).await
    }

    /// Flush all pending and return the underlying disk cache for cleanup.
    pub async fn close(&mut self) -> Result<()> {
        self.flush().await
    }

    /// Get mutable access to the active Vec (for event applier integration).
    pub fn active_mut(&mut self) -> &mut Vec<TranscriptMessage> {
        &mut self.active
    }

    /// Call after any direct mutation of the active Vec (via active_mut)
    /// to keep total count and archival in sync.
    pub fn after_mutation(&mut self) {
        self.total = self.total.max(self.base_seq + self.active.len());
        self.maybe_archive();
    }
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn style_to_str(style: TranscriptStyle) -> &'static str {
    match style {
        TranscriptStyle::User => "user",
        TranscriptStyle::Thinking => "thinking",
        TranscriptStyle::Assistant => "assistant",
        TranscriptStyle::SkillPrompt => "skill_prompt",
        TranscriptStyle::Meta => "meta",
        TranscriptStyle::Error => "error",
        TranscriptStyle::ToolRunning => "tool_running",
        TranscriptStyle::ToolSuccess => "tool_success",
        TranscriptStyle::ToolFailed => "tool_failed",
        TranscriptStyle::StatusRunning => "status_running",
        TranscriptStyle::StatusSuccess => "status_success",
        TranscriptStyle::StatusFailed => "status_failed",
    }
}

fn str_to_style(s: &str) -> TranscriptStyle {
    match s {
        "user" => TranscriptStyle::User,
        "thinking" => TranscriptStyle::Thinking,
        "assistant" => TranscriptStyle::Assistant,
        "skill_prompt" => TranscriptStyle::SkillPrompt,
        "meta" => TranscriptStyle::Meta,
        "error" => TranscriptStyle::Error,
        "tool_running" => TranscriptStyle::ToolRunning,
        "tool_success" => TranscriptStyle::ToolSuccess,
        "tool_failed" => TranscriptStyle::ToolFailed,
        "status_running" => TranscriptStyle::StatusRunning,
        "status_success" => TranscriptStyle::StatusSuccess,
        "status_failed" => TranscriptStyle::StatusFailed,
        _ => TranscriptStyle::Meta,
    }
}

fn message_from_row(row: &turso::Row) -> Result<TranscriptMessage> {
    let style: String = row.get(1)?;
    let content: String = row.get(2)?;
    let tool_name: Option<String> = row.get(3)?;
    let tool_args: Option<String> = row.get(4)?;
    let tool_output: Option<String> = row.get(5)?;
    let tool_old: Option<String> = row.get(6)?;
    let tool_new: Option<String> = row.get(7)?;
    let tool_path: Option<String> = row.get(8)?;
    let duration: Option<f64> = row.get(9)?;
    let expanded: i64 = row.get(10)?;
    let pinned: i64 = row.get(11)?;
    let status: Option<String> = row.get(12)?;
    let indent: i64 = row.get(13)?;
    let tree: Option<String> = row.get(14)?;
    let model: Option<String> = row.get(15)?;
    let agent: Option<String> = row.get(16)?;
    let user_shell: i64 = row.get(17)?;
    let slash_resp: i64 = row.get(18)?;

    let style_enum = str_to_style(&style);
    let mut msg = if let Some(name) = tool_name {
        TranscriptMessage::tool_call(name, tool_args.unwrap_or_default(), style_enum)
    } else {
        TranscriptMessage::text(content.clone(), style_enum)
    };

    msg.content = content;
    msg.duration_secs = duration;
    msg.detail_expanded = expanded != 0;
    msg.user_pinned = pinned != 0;
    msg.status_detail = status;
    msg.status_indent = indent as u16;
    msg.tree_prefix = tree;
    msg.model_tag = model;
    msg.agent_tag = agent;
    msg.user_shell = user_shell != 0;
    msg.local_slash_response = slash_resp != 0;

    if let Some(ref mut tool) = msg.tool {
        tool.output = tool_output.unwrap_or_default();
        tool.old_text = tool_old;
        tool.new_text = tool_new;
        tool.file_path = tool_path;
    }

    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_style_conversion() {
        let styles = [
            TranscriptStyle::User,
            TranscriptStyle::Thinking,
            TranscriptStyle::Assistant,
            TranscriptStyle::ToolRunning,
            TranscriptStyle::ToolSuccess,
            TranscriptStyle::ToolFailed,
            TranscriptStyle::Meta,
            TranscriptStyle::Error,
            TranscriptStyle::StatusRunning,
            TranscriptStyle::StatusSuccess,
            TranscriptStyle::StatusFailed,
        ];
        for s in &styles {
            assert_eq!(str_to_style(style_to_str(*s)), *s, "mismatch for {s:?}");
        }
    }

    #[test]
    fn active_window_overflow_archives() {
        let mut cache = CachedTranscript::new(None);
        for i in 0..MAX_ACTIVE + ARCHIVE_BATCH {
            cache.push(TranscriptMessage::text(format!("msg {i}"), TranscriptStyle::Meta));
        }
        assert!(cache.active.len() <= MAX_ACTIVE);
        assert_eq!(cache.base_seq, ARCHIVE_BATCH);
        assert_eq!(cache.total, MAX_ACTIVE + ARCHIVE_BATCH);
        assert_eq!(cache.active[0].content, format!("msg {}", ARCHIVE_BATCH));
    }

    #[test]
    fn get_returns_none_for_archived() {
        let mut cache = CachedTranscript::new(None);
        for i in 0..MAX_ACTIVE + 1 {
            cache.push(TranscriptMessage::text(format!("msg {i}"), TranscriptStyle::Meta));
        }
        assert!(cache.get(0).is_none());
        assert!(cache.get(cache.base_seq).is_some());
    }

    #[test]
    fn total_len_tracks_all_messages() {
        let mut cache = CachedTranscript::new(None);
        for i in 0..MAX_ACTIVE + ARCHIVE_BATCH {
            cache.push(TranscriptMessage::text(format!("msg {i}"), TranscriptStyle::Meta));
        }
        assert_eq!(cache.len(), MAX_ACTIVE + ARCHIVE_BATCH);
        assert_eq!(cache.active_len(), MAX_ACTIVE);
    }

    #[test]
    fn last_mut_returns_most_recent() {
        let mut cache = CachedTranscript::new(None);
        cache.push(TranscriptMessage::text("first", TranscriptStyle::Meta));
        cache.push(TranscriptMessage::text("second", TranscriptStyle::Meta));
        assert_eq!(cache.last_mut().unwrap().content, "second");
    }

    #[test]
    fn archival_suppressed_does_not_archive() {
        let mut cache = CachedTranscript::new(None);
        cache.suppress_archival();
        for i in 0..MAX_ACTIVE + ARCHIVE_BATCH + 10 {
            cache.push(TranscriptMessage::text(format!("msg {i}"), TranscriptStyle::Meta));
        }
        assert_eq!(cache.base_seq, 0);
        assert_eq!(cache.active.len(), MAX_ACTIVE + ARCHIVE_BATCH + 10);
        cache.resume_archival();
        cache.push(TranscriptMessage::text("last", TranscriptStyle::Meta));
        assert!(cache.base_seq > 0);
        assert!(cache.active.len() <= MAX_ACTIVE);
    }
}
