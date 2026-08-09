//! Durable worker mailbox (project-scoped messages).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use turso::Connection;

use crate::datastore::{connect, with_conn};
use crate::messages::now_iso_timestamp;
use crate::session::id::create_worker_msg_id;

use super::types::{MessageKind, MessageStatus, WorkerMessage};

#[derive(Clone)]
pub struct MailboxStore {
    db_path: PathBuf,
    database: Option<Arc<turso::Database>>,
}

impl MailboxStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            database: None,
        }
    }

    pub fn with_database(mut self, database: Arc<turso::Database>) -> Self {
        self.database = Some(database);
        self
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    async fn with_conn<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Connection) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        match &self.database {
            Some(db) => {
                let conn = connect(db).await?;
                f(conn).await
            }
            None => with_conn(&self.db_path, f)
                .await
                .with_context(|| format!("open mailbox {}", self.db_path.display())),
        }
    }

    #[allow(clippy::too_many_arguments)] // message envelope fields
    pub async fn send_prompt(
        &self,
        project_key: &str,
        from_worker_id: &str,
        from_session_id: &str,
        to_session_id: &str,
        to_worker_id: Option<&str>,
        text: &str,
        hops: i64,
        parent_msg_id: Option<&str>,
        conversation_id: Option<&str>,
    ) -> Result<WorkerMessage> {
        let id = create_worker_msg_id();
        let now = now_iso_timestamp();
        let payload = serde_json::json!({ "text": text }).to_string();
        self.insert_message(WorkerMessage {
            id,
            project_key: project_key.into(),
            from_worker_id: from_worker_id.into(),
            from_session_id: from_session_id.into(),
            to_worker_id: to_worker_id.map(str::to_string),
            to_session_id: to_session_id.into(),
            kind: MessageKind::Prompt,
            status: MessageStatus::Queued,
            conversation_id: conversation_id.map(str::to_string),
            parent_msg_id: parent_msg_id.map(str::to_string),
            hops,
            payload,
            created_at: now,
            delivered_at: None,
            completed_at: None,
            error: None,
        })
        .await
    }

    #[allow(clippy::too_many_arguments)] // message envelope fields
    pub async fn send_response(
        &self,
        project_key: &str,
        from_worker_id: &str,
        from_session_id: &str,
        to_session_id: &str,
        parent_msg_id: &str,
        text: &str,
        error: Option<&str>,
    ) -> Result<WorkerMessage> {
        let id = create_worker_msg_id();
        let now = now_iso_timestamp();
        let payload = serde_json::json!({ "text": text, "error": error }).to_string();
        let status = if error.is_some() {
            MessageStatus::Error
        } else {
            MessageStatus::Complete
        };
        // Response is immediately complete for the asker to read.
        let mut msg = WorkerMessage {
            id,
            project_key: project_key.into(),
            from_worker_id: from_worker_id.into(),
            from_session_id: from_session_id.into(),
            to_worker_id: None,
            to_session_id: to_session_id.into(),
            kind: MessageKind::Response,
            status,
            conversation_id: None,
            parent_msg_id: Some(parent_msg_id.into()),
            hops: 0,
            payload,
            created_at: now.clone(),
            delivered_at: Some(now.clone()),
            completed_at: Some(now),
            error: error.map(str::to_string),
        };
        // Also mark the original prompt complete.
        self.with_conn(|conn| {
            let parent = parent_msg_id.to_string();
            let now = msg.created_at.clone();
            async move {
                conn.execute(
                    "UPDATE worker_messages SET status = 'complete', completed_at = ?
                     WHERE id = ? AND kind = 'prompt'",
                    turso::params![now.as_str(), parent.as_str()],
                )
                .await?;
                Ok(())
            }
        })
        .await?;
        msg = self.insert_message(msg).await?;
        Ok(msg)
    }

    async fn insert_message(&self, msg: WorkerMessage) -> Result<WorkerMessage> {
        self.with_conn(|conn| {
            let m = msg.clone();
            async move {
                conn.execute(
                    "INSERT INTO worker_messages (
                        id, project_key, from_worker_id, from_session_id, to_worker_id, to_session_id,
                        kind, status, conversation_id, parent_msg_id, hops, payload,
                        created_at, delivered_at, completed_at, error
                     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    turso::params![
                        m.id.as_str(),
                        m.project_key.as_str(),
                        m.from_worker_id.as_str(),
                        m.from_session_id.as_str(),
                        m.to_worker_id.as_deref(),
                        m.to_session_id.as_str(),
                        m.kind.as_str(),
                        m.status.as_str(),
                        m.conversation_id.as_deref(),
                        m.parent_msg_id.as_deref(),
                        m.hops,
                        m.payload.as_str(),
                        m.created_at.as_str(),
                        m.delivered_at.as_deref(),
                        m.completed_at.as_deref(),
                        m.error.as_deref(),
                    ],
                )
                .await?;
                Ok(m)
            }
        })
        .await
    }

    /// Atomically claim next queued prompt for a session.
    pub async fn claim_next_inbound(&self, to_session_id: &str) -> Result<Option<WorkerMessage>> {
        let now = now_iso_timestamp();
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT id FROM worker_messages
                     WHERE to_session_id = ? AND kind = 'prompt' AND status = 'queued'
                     ORDER BY created_at ASC LIMIT 1",
                    turso::params![to_session_id],
                )
                .await?;
            let Some(row) = rows.next().await? else {
                return Ok(None);
            };
            let id: String = row.get(0)?;
            while rows.next().await?.is_some() {}
            let n = conn
                .execute(
                    "UPDATE worker_messages SET status = 'delivered', delivered_at = ?
                     WHERE id = ? AND status = 'queued'",
                    turso::params![now.as_str(), id.as_str()],
                )
                .await?;
            if n == 0 {
                return Ok(None);
            }
            load_message(&conn, &id).await
        })
        .await
    }

    pub async fn get(&self, msg_id: &str) -> Result<Option<WorkerMessage>> {
        self.with_conn(|conn| async move { load_message(&conn, msg_id).await })
            .await
    }

    /// Find response for a prompt msg_id (parent).
    pub async fn get_response_for(&self, parent_msg_id: &str) -> Result<Option<WorkerMessage>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT id FROM worker_messages
                     WHERE parent_msg_id = ? AND kind = 'response'
                     ORDER BY created_at DESC LIMIT 1",
                    turso::params![parent_msg_id],
                )
                .await?;
            let Some(row) = rows.next().await? else {
                return Ok(None);
            };
            let id: String = row.get(0)?;
            while rows.next().await?.is_some() {}
            load_message(&conn, &id).await
        })
        .await
    }

    pub async fn mark_timeout(&self, msg_id: &str) -> Result<()> {
        let now = now_iso_timestamp();
        self.with_conn(|conn| async move {
            conn.execute(
                "UPDATE worker_messages SET status = 'timeout', completed_at = ?, error = 'timeout'
                 WHERE id = ? AND status IN ('queued','delivered')",
                turso::params![now.as_str(), msg_id],
            )
            .await?;
            Ok(())
        })
        .await
    }

    /// Delivered prompts awaiting a response for this session (open asks from peers).
    pub async fn list_open_delivered_prompts(&self, to_session_id: &str) -> Result<Vec<WorkerMessage>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT id FROM worker_messages
                     WHERE to_session_id = ? AND kind = 'prompt' AND status = 'delivered'
                     ORDER BY created_at ASC",
                    turso::params![to_session_id],
                )
                .await?;
            let mut ids = Vec::new();
            while let Some(row) = rows.next().await? {
                let id: String = row.get(0)?;
                ids.push(id);
            }
            let mut out = Vec::new();
            for id in ids {
                if let Some(msg) = load_message(&conn, &id).await? {
                    // Still open if no response row yet.
                    let mut resp = conn
                        .query(
                            "SELECT 1 FROM worker_messages
                             WHERE parent_msg_id = ? AND kind = 'response' LIMIT 1",
                            turso::params![id.as_str()],
                        )
                        .await?;
                    let has_resp = resp.next().await?.is_some();
                    while resp.next().await?.is_some() {}
                    if !has_resp {
                        out.push(msg);
                    }
                }
            }
            Ok(out)
        })
        .await
    }

    /// Mark timed-out prompts (queued/delivered older than `timeout_ms`) for a project.
    pub async fn sweep_timeouts(&self, project_key: &str, timeout_ms: u64) -> Result<usize> {
        if timeout_ms == 0 {
            return Ok(0);
        }
        let now = now_iso_timestamp();
        let open = self
            .with_conn(|conn| async move {
                let mut rows = conn
                    .query(
                        "SELECT id, created_at FROM worker_messages
                         WHERE project_key = ? AND kind = 'prompt'
                           AND status IN ('queued','delivered')",
                        turso::params![project_key],
                    )
                    .await?;
                let mut out = Vec::new();
                while let Some(row) = rows.next().await? {
                    let id: String = row.get(0)?;
                    let created: String = row.get(1)?;
                    out.push((id, created));
                }
                Ok(out)
            })
            .await?;
        let mut n = 0usize;
        for (id, created) in open {
            let age_ms = approx_age_ms(&created, &now);
            if age_ms >= timeout_ms as i64 {
                self.mark_timeout(&id).await?;
                n += 1;
            }
        }
        Ok(n)
    }
}

fn approx_age_ms(created: &str, now: &str) -> i64 {
    fn approx_secs(s: &str) -> i64 {
        let n = s.replace('T', " ");
        let head = n.get(..19).unwrap_or("");
        let p: Vec<&str> = head.split([' ', '-', ':']).collect();
        if p.len() < 6 {
            return 0;
        }
        let y: i64 = p[0].parse().unwrap_or(0);
        let mo: i64 = p[1].parse().unwrap_or(1);
        let d: i64 = p[2].parse().unwrap_or(1);
        let h: i64 = p[3].parse().unwrap_or(0);
        let mi: i64 = p[4].parse().unwrap_or(0);
        let se: i64 = p[5].parse().unwrap_or(0);
        y * 365 * 86400 + mo * 30 * 86400 + d * 86400 + h * 3600 + mi * 60 + se
    }
    approx_secs(now)
        .saturating_sub(approx_secs(created))
        .saturating_mul(1000)
}

async fn load_message(conn: &Connection, id: &str) -> Result<Option<WorkerMessage>> {
    let mut rows = conn
        .query(
            "SELECT id, project_key, from_worker_id, from_session_id, to_worker_id, to_session_id,
                    kind, status, conversation_id, parent_msg_id, hops, payload,
                    created_at, delivered_at, completed_at, error
             FROM worker_messages WHERE id = ?",
            turso::params![id],
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return Ok(None);
    };
    let kind_s: String = row.get(6)?;
    let status_s: String = row.get(7)?;
    let msg = WorkerMessage {
        id: row.get(0)?,
        project_key: row.get(1)?,
        from_worker_id: row.get(2)?,
        from_session_id: row.get(3)?,
        to_worker_id: row.get(4)?,
        to_session_id: row.get(5)?,
        kind: MessageKind::parse(&kind_s).unwrap_or(MessageKind::Notify),
        status: MessageStatus::parse(&status_s).unwrap_or(MessageStatus::Error),
        conversation_id: row.get(8)?,
        parent_msg_id: row.get(9)?,
        hops: row.get(10)?,
        payload: row.get(11)?,
        created_at: row.get(12)?,
        delivered_at: row.get(13)?,
        completed_at: row.get(14)?,
        error: row.get(15)?,
    };
    while rows.next().await?.is_some() {}
    Ok(Some(msg))
}
