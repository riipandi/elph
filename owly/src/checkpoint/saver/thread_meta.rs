//! Per-thread metadata (display name, auto-naming flag).

use chrono::Utc;

use super::TursoCheckpointSaver;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadMetadata {
    pub display_name: Option<String>,
    pub auto_named: bool,
}

impl TursoCheckpointSaver {
    pub(super) async fn ensure_thread_metadata_table(&self) -> anyhow::Result<()> {
        let conn = self.connection().await?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS thread_metadata (
                thread_id TEXT PRIMARY KEY,
                display_name TEXT,
                auto_named INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            );",
        )
        .await
        .map_err(|error| anyhow::anyhow!("thread_metadata schema: {error}"))?;
        Ok(())
    }

    pub async fn get_thread_metadata(&self, thread_id: &str) -> anyhow::Result<ThreadMetadata> {
        self.ensure_thread_metadata_table().await?;
        let conn = self.connection().await?;
        let mut rows = conn
            .query(
                "SELECT display_name, auto_named FROM thread_metadata WHERE thread_id = ?",
                turso::params![thread_id],
            )
            .await
            .map_err(|error| anyhow::anyhow!("thread_metadata read: {error}"))?;
        let Some(row) = rows.next().await? else {
            return Ok(ThreadMetadata::default());
        };
        let display_name: Option<String> = row.get(0)?;
        let auto_named: i64 = row.get(1)?;
        Ok(ThreadMetadata {
            display_name,
            auto_named: auto_named != 0,
        })
    }

    pub async fn set_thread_display_name(
        &self,
        thread_id: &str,
        display_name: &str,
        auto_named: bool,
    ) -> anyhow::Result<()> {
        self.ensure_thread_metadata_table().await?;
        let conn = self.connection().await?;
        let updated_at = Utc::now().to_rfc3339();
        let auto_flag = i64::from(auto_named);
        conn.execute(
            "INSERT INTO thread_metadata (thread_id, display_name, auto_named, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(thread_id) DO UPDATE SET
                display_name = excluded.display_name,
                auto_named = excluded.auto_named,
                updated_at = excluded.updated_at",
            turso::params![thread_id, display_name, auto_flag, updated_at],
        )
        .await
        .map_err(|error| anyhow::anyhow!("thread_metadata write: {error}"))?;
        Ok(())
    }

    pub async fn delete_thread_metadata(&self, thread_id: &str) -> anyhow::Result<()> {
        self.ensure_thread_metadata_table().await?;
        let conn = self.connection().await?;
        conn.execute("DELETE FROM thread_metadata WHERE thread_id = ?", turso::params![thread_id])
            .await
            .map_err(|error| anyhow::anyhow!("thread_metadata delete: {error}"))?;
        Ok(())
    }
}
