use anyhow::Result;
use turso::params;

use super::super::store::MemoryStore;
use super::super::types::{Memory, MemoryCategory, MemoryRecord};
use super::super::util::{category_from_str, drain_rows, embedding_status, vec_buf};

impl MemoryStore {
    /// List memories, optionally filtered by category.
    pub async fn list_memories(&self, category: Option<MemoryCategory>) -> Result<Vec<MemoryRecord>> {
        self.init().await?;
        let filter = category.map(super::super::util::category_str);
        self.with_db(move |conn| async move {
            let (sql, params): (String, Vec<String>) = if let Some(cat) = filter {
                (
                    "SELECT id, content, category, weight, retrieval_count, created_at, length(embedding) as emb_len FROM memories WHERE category = ? ORDER BY created_at DESC".into(),
                    vec![cat.to_string()],
                )
            } else {
                (
                    "SELECT id, content, category, weight, retrieval_count, created_at, length(embedding) as emb_len FROM memories ORDER BY created_at DESC".into(),
                    vec![],
                )
            };

            let mut rows = if params.is_empty() {
                conn.query(&sql, ()).await?
            } else {
                conn.query(&sql, params![params[0].as_str()]).await?
            };

            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(MemoryRecord {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: category_from_str(&row.get::<String>(2)?),
                    weight: row.get(3)?,
                    retrieval_count: row.get(4)?,
                    created_at: row.get(5)?,
                    embedding_status: embedding_status(row.get::<Option<i64>>(6)?, self.dimensions()),
                });
            }
            drain_rows(&mut rows).await?;
            Ok(out)
        })
        .await
    }

    /// Read-only semantic search — no task record, no retrieval side effects.
    pub async fn search_memories(&self, query: &str) -> Result<Vec<Memory>> {
        self.init().await?;
        let embedding = (self.embed_fn())(query).await?;
        let emb_buf = vec_buf(&embedding);
        let sql = self.retrieval_sql();
        let decay_rate = self.decay_rate();
        let top_k = self.top_k();
        let now = super::super::store::now_secs();

        self.with_db(move |conn| async move {
            let mut rows = conn
                .query(
                    sql.as_ref(),
                    params![emb_buf.as_slice(), emb_buf.as_slice(), decay_rate, now, top_k],
                )
                .await?;

            let mut mems = Vec::new();
            while let Some(row) = rows.next().await? {
                let distance: f64 = row.get(6)?;
                mems.push(Memory {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: category_from_str(&row.get::<String>(2)?),
                    weight: row.get(3)?,
                    score: 1.0 - distance,
                    created_at: row.get(4)?,
                    retrieval_count: row.get(5)?,
                });
            }
            drain_rows(&mut rows).await?;
            Ok(mems)
        })
        .await
    }
}
