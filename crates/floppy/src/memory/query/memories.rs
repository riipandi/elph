use anyhow::Result;
use turso::params;

use crate::core::util::{drain_rows, vec_buf};
use crate::memory::store::MemoryStore;
use crate::memory::store::embed;
use crate::memory::types::{Memory, MemoryCategory, MemoryRecord};
use crate::memory::util::{category_from_str, embedding_status};

impl MemoryStore {
    /// List memories, optionally filtered by category.
    pub async fn list_memories(&self, category: Option<MemoryCategory>) -> Result<Vec<MemoryRecord>> {
        self.init().await?;
        let filter = category.map(crate::memory::util::category_str);
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

    /// List most recent memories by `created_at`, optionally filtered by category.
    pub async fn list_recent_memories(
        &self,
        limit: u32,
        category: Option<MemoryCategory>,
    ) -> Result<Vec<MemoryRecord>> {
        self.init().await?;
        let limit = limit.max(1) as i64;
        let filter = category.map(crate::memory::util::category_str);
        self.with_db(move |conn| async move {
            let (sql, cat_param): (String, Option<String>) = if let Some(cat) = filter {
                (
                    "SELECT id, content, category, weight, retrieval_count, created_at, length(embedding) as emb_len \
                     FROM memories WHERE category = ? ORDER BY created_at DESC LIMIT ?"
                        .into(),
                    Some(cat.to_string()),
                )
            } else {
                (
                    "SELECT id, content, category, weight, retrieval_count, created_at, length(embedding) as emb_len \
                     FROM memories ORDER BY created_at DESC LIMIT ?"
                        .into(),
                    None,
                )
            };

            let mut rows = if let Some(cat) = cat_param {
                conn.query(&sql, params![cat.as_str(), limit]).await?
            } else {
                conn.query(&sql, params![limit]).await?
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

        let embedding = match (self.embed_fn())(&[query.to_string()]).await {
            Ok(vecs) => vecs.into_iter().next(),
            Err(e) => {
                log::warn!("Failed to embed search query: {e:#}, using keyword-only search");
                None
            }
        };

        let (emb_buf, _has_embedding) = match embedding {
            Some(vec) if !embed::is_zero_embedding(&vec) => (Some(vec_buf(&vec)), true),
            _ => {
                log::warn!("Empty or zero embedding for search query, using keyword-only search");
                (None, false)
            }
        };

        let decay_rate = self.decay_rate();
        let now = crate::memory::store::now_secs();

        if let Some(buf) = emb_buf {
            self.with_db(move |conn| async move {
                self.hybrid_retrieve(&conn, query, buf.as_slice(), decay_rate, now)
                    .await
            })
            .await
        } else {
            // Fallback: keyword-only search by weight when embedding fails
            self.with_db(move |conn| async move {
                let mut rows = conn
                    .query(
                        "SELECT id, content, category, weight, created_at, retrieval_count FROM memories ORDER BY weight DESC LIMIT ?",
                        turso::params![self.top_k()],
                    )
                    .await?;
                
                let mut out = Vec::new();
                while let Some(row) = rows.next().await? {
                    out.push(Memory {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        category: category_from_str(&row.get::<String>(2)?),
                        weight: row.get(3)?,
                        score: row.get(3)?, // Use weight as score for keyword-only
                        created_at: row.get(4)?,
                        retrieval_count: row.get(5)?,
                    });
                }
                drain_rows(&mut rows).await?;
                Ok(out)
            })
            .await
        }
    }
}
