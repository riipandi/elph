use anyhow::Result;
use turso::params;

use super::EMBED_PENDING_BATCH;
use super::MemoryStore;
use super::{new_id, now_secs};
use crate::core::util::{drain_rows, vec_buf};
use crate::memory::types::MemoryCategory;
use crate::memory::util::category_str;

fn is_zero_embedding(v: &[f32]) -> bool {
    v.is_empty() || v.iter().all(|x| *x == 0.0)
}

fn embedding_bytes_all_zero(emb: &[u8]) -> bool {
    if emb.len() < 4 || !emb.len().is_multiple_of(4) {
        return true;
    }
    emb.chunks_exact(4)
        .all(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) == 0.0)
}

impl MemoryStore {
    pub async fn insert_raw_memory(&self, content: &str, category: MemoryCategory, weight: f64) -> Result<String> {
        self.init().await?;
        let id = new_id();
        let now = now_secs();
        let content = content.to_string();
        let cat = category_str(category).to_string();
        let current_task = self.current_task_id.lock().unwrap().clone();

        self.with_db(move |conn| async move {
            conn.execute(
                "INSERT INTO memories (id, content, embedding, category, weight, created_at, source_task) VALUES (?, ?, NULL, ?, ?, ?, ?)",
                params![id.clone(), content, cat, weight, now, current_task],
            )
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn embed_pending(&self) -> Result<usize> {
        self.init().await?;
        let mut total = 0usize;
        loop {
            let n = self.embed_pending_batch().await?;
            if n == 0 {
                break;
            }
            total += n;
        }
        Ok(total)
    }

    async fn embed_pending_batch(&self) -> Result<usize> {
        let rows: Vec<(String, String)> = self
            .with_db(|conn| async move {
                let mut r = conn
                    .query(
                        "SELECT id, content FROM memories WHERE embedding IS NULL LIMIT ?",
                        params![EMBED_PENDING_BATCH],
                    )
                    .await?;
                let mut out = Vec::new();
                while let Some(row) = r.next().await? {
                    out.push((row.get::<String>(0)?, row.get::<String>(1)?));
                }
                drain_rows(&mut r).await?;
                Ok(out)
            })
            .await?;

        if rows.is_empty() {
            return Ok(0);
        }

        let mut embedded = Vec::with_capacity(rows.len());
        for (id, content) in &rows {
            let vec = (self.embed)(content).await?;
            // Never persist all-zero vectors (noop embedder) — they corrupt semantic search.
            if is_zero_embedding(&vec) {
                continue;
            }
            embedded.push((id.clone(), vec_buf(&vec)));
        }

        if embedded.is_empty() {
            return Ok(0);
        }

        let n = embedded.len();
        self.with_db(move |conn| async move {
            for (id, emb) in embedded {
                conn.execute("UPDATE memories SET embedding = ? WHERE id = ?", params![emb.as_slice(), id])
                    .await?;
            }
            Ok(())
        })
        .await?;

        Ok(n)
    }

    /// Clear all-zero embedding blobs left by a previous noop `embed_pending` run.
    ///
    /// Returns the number of rows reset to `embedding IS NULL` so a real model can re-embed them.
    pub async fn clear_zero_embeddings(&self) -> Result<u32> {
        self.init().await?;
        let dims = self.dimensions() as usize;
        let expected_bytes = dims * std::mem::size_of::<f32>();
        self.with_db(move |conn| async move {
            let mut rows = conn
                .query("SELECT id, embedding FROM memories WHERE embedding IS NOT NULL", ())
                .await?;
            let mut bad_ids = Vec::new();
            while let Some(row) = rows.next().await? {
                let id: String = row.get(0)?;
                let emb: Vec<u8> = row.get(1)?;
                if emb.len() != expected_bytes || embedding_bytes_all_zero(&emb) {
                    bad_ids.push(id);
                }
            }
            drain_rows(&mut rows).await?;
            let n = bad_ids.len() as u32;
            for id in bad_ids {
                conn.execute("UPDATE memories SET embedding = NULL WHERE id = ?", params![id])
                    .await?;
            }
            Ok(n)
        })
        .await
    }

    pub async fn contradict_memory(&self, memory_id: &str, correction: Option<&str>) -> Result<(bool, Option<String>)> {
        self.init().await?;
        let mid = memory_id.to_string();

        let deleted = self
            .with_db(move |conn| async move {
                let changes = conn
                    .execute("DELETE FROM memories WHERE id = ?", params![mid.clone()])
                    .await?;
                conn.execute("DELETE FROM memory_retrievals WHERE memory_id = ?", params![mid.clone()])
                    .await?;
                Ok(changes > 0)
            })
            .await?;

        let mut correction_id = None;
        if let (Some(correction), true) = (correction, deleted) {
            let embedding = (self.embed)(correction).await?;
            let emb_buf = vec_buf(&embedding);
            let id = new_id();
            let now = now_secs();
            let correction = correction.to_string();
            let current_task = self.current_task_id.lock().unwrap().clone();
            let id_clone = id.clone();

            self.with_db(move |conn| async move {
                conn.execute(
                    "INSERT INTO memories (id, content, embedding, category, weight, created_at, source_task) VALUES (?, ?, ?, 'correction', 2.0, ?, ?)",
                    params![id_clone, correction, emb_buf, now, current_task],
                )
                .await?;
                Ok(())
            })
            .await?;

            correction_id = Some(id);
        }

        Ok((deleted, correction_id))
    }
}
