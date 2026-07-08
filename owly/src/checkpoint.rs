//! Port of [`@langchain/langgraph-checkpoint-sqlite`][1] to Rust/Turso.
//!
//! [1]: https://github.com/langchain-ai/langgraphjs/tree/main/libs/checkpoint-sqlite

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

// ── Public types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub v: u8,
    pub id: String,
    pub ts: String,
    #[serde(default)]
    pub channel_values: std::collections::HashMap<String, Value>,
    #[serde(default)]
    pub channel_versions: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub versions_seen: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub source: String,
    pub step: i64,
    #[serde(default)]
    pub parents: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnableConfig {
    pub configurable: CheckpointConfigurable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfigurable {
    pub thread_id: String,
    #[serde(default)]
    pub checkpoint_ns: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_id: Option<String>,
}

pub type PendingWrite = (String, Value);

#[derive(Debug, Clone)]
pub struct CheckpointTuple {
    pub config: RunnableConfig,
    pub checkpoint: Checkpoint,
    pub metadata: Option<CheckpointMetadata>,
    pub parent_config: Option<RunnableConfig>,
    pub pending_writes: Vec<PendingWriteWithTask>,
}

#[derive(Debug, Clone)]
pub struct PendingWriteWithTask {
    pub task_id: String,
    pub channel: String,
    pub value: Value,
}

#[derive(Debug, Clone, Default)]
pub struct CheckpointListOptions {
    pub limit: Option<u64>,
    pub before: Option<RunnableConfig>,
    pub filter: Option<std::collections::HashMap<String, Value>>,
}

// ── SqliteSaver ─────────────────────────────────────────────────────────────

/// Turso-backed checkpointer, mirroring `SqliteSaver` from langgraphjs.
pub struct SqliteSaver {
    db_path: PathBuf,
}

impl SqliteSaver {
    /// Open (or create) the checkpoint DB at the given path (or default).
    pub async fn open(db_path: Option<PathBuf>) -> anyhow::Result<Self> {
        let db_path = db_path.unwrap_or_else(default_db_path);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = Self { db_path };
        s.setup().await?;
        Ok(s)
    }

    pub async fn default() -> anyhow::Result<Self> {
        Self::open(None).await
    }

    // ── low-level ────────────────────────────────────────────────────────────

    async fn conn(&self) -> anyhow::Result<turso::Connection> {
        let db = turso::Builder::new_local(self.db_path.to_str().unwrap_or(""))
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("db open: {e}"))?;
        db.connect().map_err(|e| anyhow::anyhow!("db connect: {e}"))
    }

    async fn setup(&self) -> anyhow::Result<()> {
        let conn = self.conn().await?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS checkpoints (
                thread_id TEXT NOT NULL,
                checkpoint_ns TEXT NOT NULL DEFAULT '',
                checkpoint_id TEXT NOT NULL,
                parent_checkpoint_id TEXT,
                type TEXT,
                checkpoint BLOB,
                metadata BLOB,
                PRIMARY KEY (thread_id, checkpoint_ns, checkpoint_id)
            );
            CREATE TABLE IF NOT EXISTS writes (
                thread_id TEXT NOT NULL,
                checkpoint_ns TEXT NOT NULL DEFAULT '',
                checkpoint_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                idx INTEGER NOT NULL,
                channel TEXT NOT NULL,
                type TEXT,
                value BLOB,
                PRIMARY KEY (thread_id, checkpoint_ns, checkpoint_id, task_id, idx)
            );",
        )
        .await
        .map_err(|e| anyhow::anyhow!("schema: {e}"))?;
        Ok(())
    }

    /// Parse a row from the 8-column query result into a CheckpointTuple.
    fn row_to_tuple(
        tid: String,
        ns: String,
        cid: String,
        parent_cid: Option<String>,
        checkpoint_str: String,
        metadata_str: Option<String>,
        pw_str: String,
    ) -> CheckpointTuple {
        let checkpoint: Checkpoint = serde_json::from_str(&checkpoint_str).unwrap_or_default();
        let metadata: Option<CheckpointMetadata> = metadata_str.and_then(|m| serde_json::from_str(&m).ok());
        let pending_writes = Self::parse_pending_writes(&pw_str);
        CheckpointTuple {
            config: RunnableConfig {
                configurable: CheckpointConfigurable {
                    thread_id: tid.clone(),
                    checkpoint_ns: ns.clone(),
                    checkpoint_id: Some(cid.clone()),
                },
            },
            checkpoint,
            metadata,
            parent_config: parent_cid.map(|pid| RunnableConfig {
                configurable: CheckpointConfigurable {
                    thread_id: tid,
                    checkpoint_ns: ns,
                    checkpoint_id: Some(pid),
                },
            }),
            pending_writes,
        }
    }

    fn parse_pending_writes(raw: &str) -> Vec<PendingWriteWithTask> {
        if raw.is_empty() || raw == "[]" {
            return Vec::new();
        }
        let arr: Vec<serde_json::Value> = serde_json::from_str(raw).unwrap_or_default();
        arr.into_iter()
            .filter_map(|v| {
                Some(PendingWriteWithTask {
                    task_id: v.get("task_id")?.as_str()?.to_string(),
                    channel: v.get("channel")?.as_str()?.to_string(),
                    value: v.get("value").cloned().unwrap_or(Value::Null),
                })
            })
            .collect()
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Get a single checkpoint tuple.
    pub async fn get_tuple(&self, config: &RunnableConfig) -> anyhow::Result<Option<CheckpointTuple>> {
        let conn = self.conn().await?;
        let tid = &config.configurable.thread_id;
        let ns = &config.configurable.checkpoint_ns;

        let sql = if config.configurable.checkpoint_id.is_some() {
            "SELECT
                thread_id, checkpoint_ns, checkpoint_id, parent_checkpoint_id,
                type, checkpoint, metadata,
                (SELECT COALESCE(json_group_array(
                    json_object('task_id', pw.task_id, 'channel', pw.channel, 'type', pw.type, 'value', CAST(pw.value AS TEXT))
                ), '[]') FROM writes pw
                    WHERE pw.thread_id = c.thread_id
                    AND pw.checkpoint_ns = c.checkpoint_ns
                    AND pw.checkpoint_id = c.checkpoint_id
                ) as pending_writes
            FROM checkpoints c
            WHERE thread_id = ? AND checkpoint_ns = ? AND checkpoint_id = ?
            ORDER BY checkpoint_id DESC LIMIT 1"
        } else {
            "SELECT
                thread_id, checkpoint_ns, checkpoint_id, parent_checkpoint_id,
                type, checkpoint, metadata,
                (SELECT COALESCE(json_group_array(
                    json_object('task_id', pw.task_id, 'channel', pw.channel,
                                'type', pw.type, 'value', CAST(pw.value AS TEXT))
                ), '[]') FROM writes pw
                    WHERE pw.thread_id = c.thread_id
                    AND pw.checkpoint_ns = c.checkpoint_ns
                    AND pw.checkpoint_id = c.checkpoint_id
                ) as pending_writes
            FROM checkpoints c
            WHERE thread_id = ? AND checkpoint_ns = ?
            ORDER BY checkpoint_id DESC LIMIT 1"
        };

        let mut rows = if let Some(cid) = &config.configurable.checkpoint_id {
            conn.query(sql, turso::params![tid.as_str(), ns.as_str(), cid.as_str()])
                .await
                .map_err(|e| anyhow::anyhow!("query: {e}"))?
        } else {
            conn.query(sql, turso::params![tid.as_str(), ns.as_str()])
                .await
                .map_err(|e| anyhow::anyhow!("query: {e}"))?
        };

        match rows.next().await.map_err(|e| anyhow::anyhow!("row: {e}"))? {
            Some(r) => Ok(Some(Self::row_to_tuple(
                r.get::<String>(0).unwrap_or_default(),
                r.get::<String>(1).unwrap_or_default(),
                r.get::<String>(2).unwrap_or_default(),
                r.get::<Option<String>>(3).ok().flatten(),
                r.get::<String>(5).unwrap_or_default(),
                r.get::<Option<String>>(6).ok().flatten(),
                r.get::<String>(7).unwrap_or_default(),
            ))),
            None => Ok(None),
        }
    }

    /// Store a checkpoint.
    pub async fn put(
        &self,
        config: &RunnableConfig,
        checkpoint: &Checkpoint,
        metadata: &CheckpointMetadata,
    ) -> anyhow::Result<RunnableConfig> {
        let conn = self.conn().await?;
        let chk_json = serde_json::to_string(checkpoint)?;
        let meta_json = serde_json::to_string(metadata)?;

        conn.execute(
            "INSERT OR REPLACE INTO checkpoints
             (thread_id, checkpoint_ns, checkpoint_id, parent_checkpoint_id, type, checkpoint, metadata)
             VALUES (?, ?, ?, ?, 'json', ?, ?)",
            turso::params![
                config.configurable.thread_id.as_str(),
                config.configurable.checkpoint_ns.as_str(),
                checkpoint.id.as_str(),
                config.configurable.checkpoint_id.as_deref().unwrap_or(""),
                chk_json.as_str(),
                meta_json.as_str(),
            ],
        )
        .await
        .map_err(|e| anyhow::anyhow!("put: {e}"))?;

        Ok(RunnableConfig {
            configurable: CheckpointConfigurable {
                thread_id: config.configurable.thread_id.clone(),
                checkpoint_ns: config.configurable.checkpoint_ns.clone(),
                checkpoint_id: Some(checkpoint.id.clone()),
            },
        })
    }

    /// Store intermediate writes.
    pub async fn put_writes(
        &self,
        config: &RunnableConfig,
        writes: &[PendingWrite],
        task_id: &str,
    ) -> anyhow::Result<()> {
        let conn = self.conn().await?;
        let tid = &config.configurable.thread_id;
        let ns = &config.configurable.checkpoint_ns;
        let cid = config
            .configurable
            .checkpoint_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing checkpoint_id"))?;

        for (idx, (channel, value)) in writes.iter().enumerate() {
            let serialized = serde_json::to_string(value)?;
            conn.execute(
                "INSERT OR REPLACE INTO writes
                 (thread_id, checkpoint_ns, checkpoint_id, task_id, idx, channel, type, value)
                 VALUES (?, ?, ?, ?, ?, ?, 'json', ?)",
                turso::params![
                    tid.as_str(),
                    ns.as_str(),
                    cid,
                    task_id,
                    idx as i64,
                    channel.as_str(),
                    serialized.as_str(),
                ],
            )
            .await
            .map_err(|e| anyhow::anyhow!("put_writes: {e}"))?;
        }
        Ok(())
    }

    /// List checkpoints for a thread.
    pub async fn list(
        &self,
        config: &RunnableConfig,
        options: &CheckpointListOptions,
    ) -> anyhow::Result<Vec<CheckpointTuple>> {
        let conn = self.conn().await?;
        let tid = &config.configurable.thread_id;
        let ns = &config.configurable.checkpoint_ns;
        let mut results = Vec::new();

        let sql = "SELECT
            thread_id, checkpoint_ns, checkpoint_id, parent_checkpoint_id,
            type, checkpoint, metadata,
            (SELECT COALESCE(json_group_array(
                json_object('task_id', pw.task_id, 'channel', pw.channel, 'type', pw.type, 'value', CAST(pw.value AS TEXT))
            ), '[]') FROM writes pw
                WHERE pw.thread_id = c.thread_id
                AND pw.checkpoint_ns = c.checkpoint_ns
                AND pw.checkpoint_id = c.checkpoint_id
            ) as pending_writes
        FROM checkpoints c
        WHERE thread_id = ? AND checkpoint_ns = ?
        ORDER BY checkpoint_id DESC";

        let mut rows = conn
            .query(sql, turso::params![tid.as_str(), ns.as_str()])
            .await
            .map_err(|e| anyhow::anyhow!("list query: {e}"))?;

        while let Some(r) = rows.next().await.map_err(|e| anyhow::anyhow!("row: {e}"))? {
            let tuple = Self::row_to_tuple(
                r.get::<String>(0).unwrap_or_default(),
                r.get::<String>(1).unwrap_or_default(),
                r.get::<String>(2).unwrap_or_default(),
                r.get::<Option<String>>(3).ok().flatten(),
                r.get::<String>(5).unwrap_or_default(),
                r.get::<Option<String>>(6).ok().flatten(),
                r.get::<String>(7).unwrap_or_default(),
            );

            // Apply filter
            if let Some(filter) = &options.filter {
                let Some(ref meta) = tuple.metadata else { continue };
                let Ok(meta_val) = serde_json::to_value(meta) else {
                    continue;
                };
                if !filter.iter().all(|(k, v)| meta_val.get(k) == Some(v)) {
                    continue;
                }
            }

            results.push(tuple);
        }

        // Apply before/limit
        if let Some(before) = &options.before
            && let Some(cid) = &before.configurable.checkpoint_id
        {
            results.retain(|t| {
                t.config
                    .configurable
                    .checkpoint_id
                    .as_deref()
                    .is_some_and(|id| id < cid.as_str())
            });
        }
        if let Some(limit) = options.limit {
            results.truncate(limit as usize);
        }

        Ok(results)
    }

    /// Delete all data for a thread.
    pub async fn delete_thread(&self, thread_id: &str) -> anyhow::Result<()> {
        let conn = self.conn().await?;
        conn.execute("DELETE FROM checkpoints WHERE thread_id = ?", turso::params![thread_id])
            .await
            .map_err(|e| anyhow::anyhow!("delete ckpt: {e}"))?;
        conn.execute("DELETE FROM writes WHERE thread_id = ?", turso::params![thread_id])
            .await
            .map_err(|e| anyhow::anyhow!("delete writes: {e}"))?;
        Ok(())
    }

    pub fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }
}

impl Default for Checkpoint {
    fn default() -> Self {
        Self {
            v: 4,
            id: elph_agent::uuidv7(),
            ts: chrono::Utc::now().to_rfc3339(),
            channel_values: std::collections::HashMap::new(),
            channel_versions: std::collections::HashMap::new(),
            versions_seen: std::collections::HashMap::new(),
        }
    }
}

fn default_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".owly").join("checkpoints.db")
}
