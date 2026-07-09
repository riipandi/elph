//! Port of [`@langchain/langgraph-checkpoint-sqlite`][1] to Rust/Turso.
//!
//! [1]: https://github.com/langchain-ai/langgraphjs/tree/main/libs/checkpoint-sqlite

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use turso::{Builder, Connection, Database};

// ── LangGraph channel constants ─────────────────────────────────────────────

pub const TASKS: &str = "__pregel_tasks";
pub const ERROR: &str = "__error__";
pub const SCHEDULED: &str = "__scheduled__";
pub const INTERRUPT: &str = "__interrupt__";
pub const RESUME: &str = "__resume__";
/// Streaming assistant draft (OR REPLACE) for mid-turn crash recovery.
pub const ASSISTANT_DRAFT: &str = "__assistant_draft__";
/// Latest partial tool output (OR REPLACE) during streaming tool execution.
pub const TOOL_PARTIAL: &str = "__tool_partial__";

/// Special write channels map to fixed negative indices (langgraph-checkpoint contract).
pub fn writes_idx(channel: &str) -> Option<i64> {
    match channel {
        ERROR => Some(-1),
        SCHEDULED => Some(-2),
        INTERRUPT => Some(-3),
        RESUME => Some(-4),
        ASSISTANT_DRAFT => Some(-5),
        TOOL_PARTIAL => Some(-6),
        _ => None,
    }
}

// ── Public types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub v: u8,
    pub id: String,
    pub ts: String,
    #[serde(default)]
    pub channel_values: HashMap<String, Value>,
    #[serde(default)]
    pub channel_versions: HashMap<String, String>,
    #[serde(default)]
    pub versions_seen: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub source: String,
    pub step: i64,
    #[serde(default)]
    pub parents: HashMap<String, String>,
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
pub type CheckpointPendingWrite = (String, String, Value);

#[derive(Debug, Clone)]
pub struct CheckpointTuple {
    pub config: RunnableConfig,
    pub checkpoint: Checkpoint,
    pub metadata: Option<CheckpointMetadata>,
    pub parent_config: Option<RunnableConfig>,
    pub pending_writes: Vec<CheckpointPendingWrite>,
}

#[derive(Debug, Clone, Default)]
pub struct CheckpointListOptions {
    pub limit: Option<u64>,
    pub before: Option<RunnableConfig>,
    pub filter: Option<HashMap<String, Value>>,
}

// ── TursoCheckpointSaver ────────────────────────────────────────────────────

/// Turso-backed checkpointer mirroring `SqliteSaver` from langgraphjs.
pub struct TursoCheckpointSaver {
    db: Database,
    db_path: PathBuf,
    is_setup: AtomicBool,
}

/// Backward-compatible alias — storage is Turso, not system SQLite.
pub type SqliteSaver = TursoCheckpointSaver;

impl TursoCheckpointSaver {
    /// Open (or create) the checkpoint DB at the given path (or default).
    pub async fn open(db_path: Option<PathBuf>) -> anyhow::Result<Self> {
        let db_path = db_path.unwrap_or_else(default_db_path);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
            secure_dir(parent)?;
        }
        let db = Builder::new_local(db_path.to_string_lossy().as_ref())
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("db open: {e}"))?;
        let saver = Self {
            db,
            db_path: db_path.clone(),
            is_setup: AtomicBool::new(false),
        };
        saver.setup().await?;
        secure_file(&db_path)?;
        Ok(saver)
    }

    pub async fn default() -> anyhow::Result<Self> {
        Self::open(None).await
    }

    async fn connection(&self) -> anyhow::Result<Connection> {
        self.db.connect().map_err(|e| anyhow::anyhow!("db connect: {e}"))
    }

    async fn setup(&self) -> anyhow::Result<()> {
        if self.is_setup.load(Ordering::Acquire) {
            return Ok(());
        }
        let conn = self.connection().await?;
        // PRAGMA returns rows — must not run inside execute_batch (Turso/libsql).
        let _ = conn.execute("PRAGMA journal_mode=WAL", ()).await;
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
        self.is_setup.store(true, Ordering::Release);
        Ok(())
    }

    const CHECKPOINT_SELECT: &'static str = "SELECT
        thread_id,
        checkpoint_ns,
        checkpoint_id,
        parent_checkpoint_id,
        type,
        checkpoint,
        metadata,
        (
            SELECT COALESCE(json_group_array(
                json_object(
                    'task_id', pw.task_id,
                    'channel', pw.channel,
                    'type', pw.type,
                    'value', CAST(pw.value AS TEXT)
                )
            ), '[]')
            FROM writes AS pw
            WHERE pw.thread_id = checkpoints.thread_id
                AND pw.checkpoint_ns = checkpoints.checkpoint_ns
                AND pw.checkpoint_id = checkpoints.checkpoint_id
            ORDER BY pw.task_id, pw.idx
        ) AS pending_writes,
        (
            SELECT COALESCE(json_group_array(
                json_object(
                    'type', ps.type,
                    'value', CAST(ps.value AS TEXT)
                )
            ), '[]')
            FROM writes AS ps
            WHERE ps.thread_id = checkpoints.thread_id
                AND ps.checkpoint_ns = checkpoints.checkpoint_ns
                AND ps.checkpoint_id = checkpoints.parent_checkpoint_id
                AND ps.channel = ?
            ORDER BY ps.idx
        ) AS pending_sends";

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

    fn parse_pending_writes(raw: &str) -> Vec<CheckpointPendingWrite> {
        if raw.is_empty() || raw == "[]" {
            return Vec::new();
        }
        let arr: Vec<Value> = serde_json::from_str(raw).unwrap_or_default();
        arr.into_iter()
            .filter_map(|v| {
                let raw = v.get("value")?;
                Some((
                    v.get("task_id")?.as_str()?.to_string(),
                    v.get("channel")?.as_str()?.to_string(),
                    decode_write_value(raw),
                ))
            })
            .collect()
    }

    async fn hydrate_tuple(&self, mut tuple: CheckpointTuple) -> anyhow::Result<CheckpointTuple> {
        if tuple.checkpoint.v < 4
            && let Some(parent_id) = tuple
                .parent_config
                .as_ref()
                .and_then(|c| c.configurable.checkpoint_id.clone())
        {
            self.migrate_pending_sends(&mut tuple.checkpoint, &tuple.config.configurable.thread_id, &parent_id)
                .await?;
        }
        Ok(tuple)
    }

    async fn migrate_pending_sends(
        &self,
        checkpoint: &mut Checkpoint,
        thread_id: &str,
        parent_checkpoint_id: &str,
    ) -> anyhow::Result<()> {
        let conn = self.connection().await?;
        let mut rows = conn
            .query(
                "SELECT COALESCE(json_group_array(
                    json_object('type', ps.type, 'value', CAST(ps.value AS TEXT))
                ), '[]') AS pending_sends
                FROM writes AS ps
                WHERE ps.thread_id = ?
                    AND ps.checkpoint_id = ?
                    AND ps.channel = ?
                ORDER BY ps.idx",
                turso::params![thread_id, parent_checkpoint_id, TASKS],
            )
            .await
            .map_err(|e| anyhow::anyhow!("migrate pending sends: {e}"))?;

        let Some(row) = rows.next().await.map_err(|e| anyhow::anyhow!("row: {e}"))? else {
            return Ok(());
        };
        let pending_sends: String = row.get(0).unwrap_or_else(|_| "[]".to_string());
        let sends: Vec<Value> = serde_json::from_str(&pending_sends).unwrap_or_default();
        if sends.is_empty() {
            return Ok(());
        }

        let values: Vec<Value> = sends
            .into_iter()
            .filter_map(|entry| entry.get("value").cloned())
            .collect();
        checkpoint
            .channel_values
            .insert(TASKS.to_string(), Value::Array(values));
        checkpoint.channel_versions.insert(
            TASKS.to_string(),
            max_channel_version(checkpoint.channel_versions.values().cloned().collect()),
        );
        Ok(())
    }

    /// Get a single checkpoint tuple.
    pub async fn get_tuple(&self, config: &RunnableConfig) -> anyhow::Result<Option<CheckpointTuple>> {
        let conn = self.connection().await?;
        let tid = &config.configurable.thread_id;
        let ns = &config.configurable.checkpoint_ns;

        let sql = if config.configurable.checkpoint_id.is_some() {
            format!(
                "{} FROM checkpoints WHERE thread_id = ? AND checkpoint_ns = ? AND checkpoint_id = ?",
                Self::CHECKPOINT_SELECT
            )
        } else {
            format!(
                "{} FROM checkpoints WHERE thread_id = ? AND checkpoint_ns = ? ORDER BY checkpoint_id DESC LIMIT 1",
                Self::CHECKPOINT_SELECT
            )
        };

        let mut rows = if let Some(cid) = &config.configurable.checkpoint_id {
            conn.query(&sql, turso::params![TASKS, tid.as_str(), ns.as_str(), cid.as_str()])
                .await
                .map_err(|e| anyhow::anyhow!("query: {e}"))?
        } else {
            conn.query(&sql, turso::params![TASKS, tid.as_str(), ns.as_str()])
                .await
                .map_err(|e| anyhow::anyhow!("query: {e}"))?
        };

        let Some(row) = rows.next().await.map_err(|e| anyhow::anyhow!("row: {e}"))? else {
            return Ok(None);
        };

        let mut tuple = Self::row_to_tuple(
            row.get::<String>(0).unwrap_or_default(),
            row.get::<String>(1).unwrap_or_default(),
            row.get::<String>(2).unwrap_or_default(),
            row.get::<Option<String>>(3).ok().flatten(),
            row.get::<String>(5).unwrap_or_default(),
            row.get::<Option<String>>(6).ok().flatten(),
            row.get::<String>(7).unwrap_or_default(),
        );

        if config.configurable.checkpoint_id.is_none() {
            tuple.config = RunnableConfig {
                configurable: CheckpointConfigurable {
                    thread_id: tuple.config.configurable.thread_id.clone(),
                    checkpoint_ns: ns.clone(),
                    checkpoint_id: tuple.config.configurable.checkpoint_id.clone(),
                },
            };
        }

        Ok(Some(self.hydrate_tuple(tuple).await?))
    }

    /// Store a checkpoint.
    pub async fn put(
        &self,
        config: &RunnableConfig,
        checkpoint: &Checkpoint,
        metadata: &CheckpointMetadata,
    ) -> anyhow::Result<RunnableConfig> {
        if config.configurable.thread_id.is_empty() {
            anyhow::bail!(r#"Missing "thread_id" field in passed "config.configurable"."#);
        }

        let conn = self.connection().await?;
        let parent_checkpoint_id = config.configurable.checkpoint_id.as_deref();
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
                parent_checkpoint_id,
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
        if config.configurable.thread_id.is_empty() {
            anyhow::bail!("Missing thread_id field in config.configurable.");
        }
        let cid = config
            .configurable
            .checkpoint_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing checkpoint_id"))?;

        let all_special = writes.iter().all(|(channel, _)| writes_idx(channel).is_some());
        let insert_mode = if all_special { "OR REPLACE" } else { "OR IGNORE" };
        let sql = format!(
            "INSERT {insert_mode} INTO writes
             (thread_id, checkpoint_ns, checkpoint_id, task_id, idx, channel, type, value)
             VALUES (?, ?, ?, ?, ?, ?, 'json', ?)"
        );

        let conn = self.connection().await?;
        let tid = &config.configurable.thread_id;
        let ns = &config.configurable.checkpoint_ns;

        for (idx, (channel, value)) in writes.iter().enumerate() {
            let serialized = serde_json::to_string(value)?;
            let write_idx = writes_idx(channel).unwrap_or(idx as i64);
            conn.execute(
                &sql,
                turso::params![
                    tid.as_str(),
                    ns.as_str(),
                    cid,
                    task_id,
                    write_idx,
                    channel.as_str(),
                    serialized.as_str(),
                ],
            )
            .await
            .map_err(|e| anyhow::anyhow!("put_writes: {e}"))?;
        }
        Ok(())
    }

    /// List checkpoints for a thread (or all threads when `thread_id` is absent).
    pub async fn list(
        &self,
        config: &RunnableConfig,
        options: &CheckpointListOptions,
    ) -> anyhow::Result<Vec<CheckpointTuple>> {
        let conn = self.connection().await?;
        let mut sql = format!("{}\nFROM checkpoints", Self::CHECKPOINT_SELECT);
        let mut where_parts = Vec::new();
        let mut bind_values: Vec<String> = vec![TASKS.to_string()];

        if !config.configurable.thread_id.is_empty() {
            where_parts.push("thread_id = ?".to_string());
            bind_values.push(config.configurable.thread_id.clone());
        }

        where_parts.push("checkpoint_ns = ?".to_string());
        bind_values.push(config.configurable.checkpoint_ns.clone());

        if let Some(before) = &options.before
            && let Some(cid) = &before.configurable.checkpoint_id
        {
            where_parts.push("checkpoint_id < ?".to_string());
            bind_values.push(cid.clone());
        }

        let sanitized_filter: Vec<(String, Value)> = options
            .filter
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, value)| !value.is_null())
            .collect();
        for (key, value) in &sanitized_filter {
            where_parts.push("json_extract(CAST(metadata AS TEXT), ?) = ?".to_string());
            bind_values.push(format!("$.{key}"));
            bind_values.push(filter_bind_value(value));
        }

        if !where_parts.is_empty() {
            sql.push_str("\nWHERE ");
            sql.push_str(&where_parts.join("\n  AND "));
        }

        sql.push_str("\nORDER BY checkpoint_id DESC");

        if let Some(limit) = options.limit {
            sql.push_str(&format!("\nLIMIT {limit}"));
        }

        let mut rows = conn
            .query(&sql, turso::params_from_iter(bind_values.iter().map(String::as_str)))
            .await
            .map_err(|e| anyhow::anyhow!("list query: {e}"))?;

        let mut results = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| anyhow::anyhow!("row: {e}"))? {
            let tuple = Self::row_to_tuple(
                row.get::<String>(0).unwrap_or_default(),
                row.get::<String>(1).unwrap_or_default(),
                row.get::<String>(2).unwrap_or_default(),
                row.get::<Option<String>>(3).ok().flatten(),
                row.get::<String>(5).unwrap_or_default(),
                row.get::<Option<String>>(6).ok().flatten(),
                row.get::<String>(7).unwrap_or_default(),
            );
            results.push(self.hydrate_tuple(tuple).await?);
        }

        Ok(results)
    }

    /// Delete all data for a thread.
    pub async fn delete_thread(&self, thread_id: &str) -> anyhow::Result<()> {
        let conn = self.connection().await?;
        conn.execute("DELETE FROM checkpoints WHERE thread_id = ?", turso::params![thread_id])
            .await
            .map_err(|e| anyhow::anyhow!("delete ckpt: {e}"))?;
        conn.execute("DELETE FROM writes WHERE thread_id = ?", turso::params![thread_id])
            .await
            .map_err(|e| anyhow::anyhow!("delete writes: {e}"))?;
        Ok(())
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

impl Default for Checkpoint {
    fn default() -> Self {
        Self {
            v: 4,
            id: elph_agent::uuidv7(),
            ts: chrono::Utc::now().to_rfc3339(),
            channel_values: HashMap::new(),
            channel_versions: HashMap::new(),
            versions_seen: HashMap::new(),
        }
    }
}

fn decode_write_value(raw: &Value) -> Value {
    if let Some(text) = raw.as_str()
        && let Ok(parsed) = serde_json::from_str::<Value>(text)
    {
        return parsed;
    }
    raw.clone()
}

fn filter_bind_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn compare_channel_versions(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<i64>(), b.parse::<i64>()) {
        (Ok(a_num), Ok(b_num)) => a_num.cmp(&b_num),
        _ => a.cmp(b),
    }
}

fn max_channel_version(versions: Vec<String>) -> String {
    versions
        .into_iter()
        .max_by(|a, b| compare_channel_versions(a.as_str(), b.as_str()))
        .unwrap_or_else(|| "1".to_string())
}

#[cfg(unix)]
fn secure_dir(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn secure_dir(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn secure_file(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_file(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

/// Default checkpoint DB path (`~/.owly/owly.sqlite`), mirroring OpenWiki's layout.
pub fn default_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".owly").join("owly.sqlite")
}

/// Copy a checkpoint for persistence (shallow channel map copy).
pub fn copy_checkpoint(checkpoint: &Checkpoint) -> Checkpoint {
    Checkpoint {
        v: checkpoint.v,
        id: checkpoint.id.clone(),
        ts: checkpoint.ts.clone(),
        channel_values: checkpoint.channel_values.clone(),
        channel_versions: checkpoint.channel_versions.clone(),
        versions_seen: checkpoint.versions_seen.clone(),
    }
}
