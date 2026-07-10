use serde_json::Value;

use super::TursoCheckpointSaver;
use crate::checkpoint::TASKS;
use crate::checkpoint::types::Checkpoint;
use crate::checkpoint::util::max_channel_version;

impl TursoCheckpointSaver {
    pub(super) async fn migrate_pending_sends(
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
}
