use anyhow::Result;
use turso::params;

use super::{MemoryStore, delete_orphan_retrievals};
use crate::floppy::types::DecayResult;

impl MemoryStore {
    pub async fn decay(&self) -> Result<DecayResult> {
        self.init().await?;
        let decay_rate = self.decay_rate;
        self.with_db(move |conn| async move {
            let decayed = conn
                .execute("UPDATE memories SET weight = weight * ?", params![decay_rate])
                .await?;
            let deleted = conn
                .execute("DELETE FROM memories WHERE weight < 0.15 AND retrieval_count > 5", ())
                .await?;
            delete_orphan_retrievals(&conn).await?;
            Ok(DecayResult {
                decayed: decayed as u32,
                deleted: deleted as u32,
            })
        })
        .await
    }

    pub async fn purge(&self, threshold: f64) -> Result<u32> {
        self.init().await?;
        self.with_db(move |conn| async move {
            let n = conn
                .execute("DELETE FROM memories WHERE weight < ?", params![threshold])
                .await?;
            delete_orphan_retrievals(&conn).await?;
            Ok(n as u32)
        })
        .await
    }
    pub async fn penalize_memory(&self, memory_id: &str, factor: f64) -> Result<()> {
        self.init().await?;
        let mid = memory_id.to_string();
        self.with_db(move |conn| async move {
            conn.execute(
                "UPDATE memories SET weight = MAX(weight * ?, 0.1) WHERE id = ?",
                params![factor, mid],
            )
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn close(&self) -> Result<()> {
        // No persistent conn — with_db opens/closes per op.
        *self.initialized.lock().unwrap() = false;
        Ok(())
    }
}
