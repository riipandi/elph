use super::TursoCheckpointSaver;
use crate::checkpoint::types::{Checkpoint, CheckpointConfigurable, CheckpointMetadata, PendingWrite, RunnableConfig};
use crate::checkpoint::writes_idx;

impl TursoCheckpointSaver {
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
}
