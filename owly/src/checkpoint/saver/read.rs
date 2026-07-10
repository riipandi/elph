use serde_json::Value;

use super::TursoCheckpointSaver;
use crate::checkpoint::TASKS;
use crate::checkpoint::types::{
    Checkpoint, CheckpointConfigurable, CheckpointListOptions, CheckpointMetadata, CheckpointPendingWrite,
    CheckpointTuple, RunnableConfig,
};
use crate::checkpoint::util::{decode_write_value, filter_bind_value};

impl TursoCheckpointSaver {
    pub(super) const CHECKPOINT_SELECT: &'static str = "SELECT
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

    pub(super) fn row_to_tuple(
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

    pub(super) async fn hydrate_tuple(&self, mut tuple: CheckpointTuple) -> anyhow::Result<CheckpointTuple> {
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
}
