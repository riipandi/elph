//! Disk-backed transcript archive using Turso (local SQLite).
//!
//! When the in-memory transcript grows past `MAX_MESSAGES_BEFORE_ARCHIVE`, the
//! shell archives the oldest messages here — one database file per project,
//! partitioned by `session_id`. The live TUI keeps working from the in-memory
//! `Vec<TranscriptMessage>`; this store is append-only for now.

use std::path::Path;

use anyhow::Result;
use turso::{Connection, params};
use turso_db::{connect, open_local};

use super::types::{TranscriptMessage, TranscriptStyle};

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
        let db = open_local(db_path, |b| b.experimental_multiprocess_wal(true), false).await?;
        let conn = connect(&db).await?;
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

    /// Insert a batch of archived messages inside a single transaction.
    ///
    /// Wrapping the batch in `BEGIN`/`COMMIT` turns N individual fsyncs into one,
    /// cutting archive latency from ~200 ms to ~20 ms for a 130-message batch.
    pub async fn push_batch(&self, batch: impl IntoIterator<Item = (usize, &TranscriptMessage)>) -> Result<()> {
        let sql = "INSERT OR IGNORE INTO transcript_messages \
                    (session_id, seq, style, content, \
                     tool_name, tool_args, tool_output, tool_old, tool_new, tool_path, \
                     duration, expanded, pinned, status, indent, tree, model, agent, \
                     user_shell, slash_resp) \
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, \
                            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)";

        self.conn.execute("BEGIN", ()).await?;
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
        self.conn.execute("COMMIT", ()).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

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
