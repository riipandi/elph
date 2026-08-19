use std::path::Path;

use anyhow::Result;
use elph_agent::datastore::{connect, open_local_with, with_write_transaction};
use turso::{Connection, params};

use super::types::{TranscriptMessage, TranscriptStyle};

pub struct TranscriptCache {
    conn: Connection,
    session_id: String,
}

impl TranscriptCache {
    pub async fn open(db_path: &Path, session_id: &str) -> Result<Self> {
        let db =
            open_local_with(db_path, |b| b.experimental_multiprocess_wal(cfg!(not(target_os = "windows")))).await?;
        let conn = connect(&db).await?;
        let cache = Self {
            conn,
            session_id: session_id.to_string(),
        };
        if let Err(err) = cache.prune_session_tree_snapshots().await {
            log::debug!("snapshot prune skipped (table may not exist yet): {err:#}");
        }
        Ok(cache)
    }

    pub async fn save_snapshot(&self, snapshot_json: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO transcript_snapshot (session_id, data, saved_at) VALUES (?1, ?2, datetime('now')) \
                 ON CONFLICT(session_id) DO UPDATE SET data = excluded.data, saved_at = excluded.saved_at",
                params![self.session_id.as_str(), snapshot_json],
            )
            .await?;
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub async fn load_snapshot(&self) -> Result<Option<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT data FROM transcript_snapshot WHERE session_id = ?1",
                params![self.session_id.as_str()],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            return Ok(Some(row.get(0)?));
        }
        Ok(None)
    }

    pub async fn prune_session_tree_snapshots(&self) -> Result<usize> {
        self.conn
            .execute(
                "DELETE FROM session_entries WHERE type = 'custom' AND payload LIKE '%elph.transcript.snapshot%'",
                (),
            )
            .await?;
        let mut rows = self.conn.query(
            "SELECT COUNT(*) FROM session_entries WHERE type = 'custom' AND payload LIKE '%elph.transcript.snapshot%'",
            (),
        ).await?;
        let remaining: i64 = rows.next().await?.map(|r| r.get(0).unwrap_or(0)).unwrap_or(0);
        if remaining == 0 {
            log::info!("pruned all legacy transcript snapshots from session tree");
        }
        Ok(0)
    }

    #[allow(dead_code)]
    pub async fn push_batch(&self, batch: impl IntoIterator<Item = (usize, &TranscriptMessage)>) -> Result<()> {
        let batch: Vec<_> = batch.into_iter().collect();
        let sql = "INSERT OR IGNORE INTO transcript_messages \
                   (session_id, seq, style, content, tool_name, tool_args, tool_output, tool_old, tool_new, tool_path, \
                    duration, expanded, pinned, status, indent, tree, model, agent, user_shell, slash_resp) \
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)";
        with_write_transaction(&self.conn, || async {
            for (seq, msg) in &batch {
                self.conn
                    .execute(
                        sql,
                        params![
                            self.session_id.as_str(),
                            *seq as i64,
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
            Ok::<(), anyhow::Error>(())
        })
        .await
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use turso::Database;

    async fn setup(path: &Path) -> Database {
        let db = open_local_with(path, |b| b.experimental_multiprocess_wal(cfg!(not(target_os = "windows"))))
            .await
            .expect("open db");
        let conn = connect(&db).await.expect("connect");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS transcript_snapshot (session_id TEXT PRIMARY KEY, data TEXT NOT NULL, saved_at TEXT NOT NULL DEFAULT (datetime('now')));").await.expect("create table");
        db
    }

    #[tokio::test]
    async fn save_snapshot_overwrites_prior() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("test.db");
        let _db = setup(&path).await;
        let cache = TranscriptCache::open(&path, "sess-1").await.expect("open");
        let first = r#"{"version":1,"messages":[{"content":"first","style":"user"}]}"#;
        cache.save_snapshot(first).await.expect("save first");
        let second = r#"{"version":1,"messages":[{"content":"second","style":"user"}]}"#;
        cache.save_snapshot(second).await.expect("save second");
        assert_eq!(cache.load_snapshot().await.expect("load").as_deref(), Some(second));
    }

    #[tokio::test]
    async fn load_snapshot_returns_none_when_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("test.db");
        let _db = setup(&path).await;
        let cache = TranscriptCache::open(&path, "sess-empty").await.expect("open");
        assert!(cache.load_snapshot().await.expect("load").is_none());
    }
}
