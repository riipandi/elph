//! Hybrid keyword + vector retrieval for the memory store.

use std::collections::HashMap;

use anyhow::Result;
use turso::{Connection, params};

use crate::core::fts::sanitize_query;
use crate::core::util::drain_rows;
use crate::memory::migrations;
use crate::memory::store::MemoryStore;
use crate::memory::types::Memory;
use crate::memory::util::category_from_str;

/// Cosine similarity (0–1) for FTS-only ids that have embeddings.
///
/// Per-id indexed lookups; the candidate window is small (`top_k * 4`, capped).
async fn keyword_cosines(conn: &Connection, ids: &[String], emb_buf: &[u8], vfn: &str) -> Result<HashMap<String, f64>> {
    let mut out = HashMap::with_capacity(ids.len());
    for id in ids {
        let mut rows = conn
            .query(
                &format!(
                    "SELECT vector_distance_cos({vfn}(embedding), {vfn}(?)) FROM memories \
                     WHERE id = ? AND embedding IS NOT NULL"
                ),
                params![emb_buf, id.as_str()],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            let distance: f64 = row.get(0)?;
            out.insert(id.clone(), 1.0 - distance);
        }
        drain_rows(&mut rows).await?;
    }
    Ok(out)
}

/// Rank-based keyword confidence for FTS-only hits without embeddings.
///
/// Decays from 0.55 (top hit) to a 0.35 floor: strong enough to pass the low
/// end of the adaptive-recall threshold, never high enough to outrank a
/// genuine semantic match.
fn keyword_score(rank: usize) -> f64 {
    (0.55 * (1.0 - 0.15 * rank as f64)).max(0.35)
}

impl MemoryStore {
    /// Hybrid keyword + vector retrieval, preserving `Memory.score` as cosine
    /// similarity (0–1).
    ///
    /// The vector path (cosine, recency-decayed) always runs. When the
    /// Turso-native FTS index is available, a keyword pass on `content` adds
    /// exact matches the vector search missed: hits that have embeddings keep
    /// their real cosine score; hits without embeddings (pending/truncated)
    /// get a rank-based keyword score in [0.35, 0.55] so they stay below strong
    /// semantic matches but above the adaptive-recall floor.
    pub(crate) async fn hybrid_retrieve(
        &self,
        conn: &Connection,
        query: &str,
        emb_buf: &[u8],
        decay_rate: f64,
        now: i64,
    ) -> Result<Vec<Memory>> {
        let top_k = self.top_k();
        let vfn = self.vector_fn();
        let sql = self.retrieval_sql();
        let mut by_id: HashMap<String, Memory> = HashMap::new();

        // Vector path (existing semantics).
        {
            let mut rows = conn
                .query(sql.as_ref(), params![emb_buf, emb_buf, decay_rate, now, top_k])
                .await?;
            while let Some(row) = rows.next().await? {
                let distance: f64 = row.get(6)?;
                by_id.insert(
                    row.get::<String>(0)?,
                    Memory {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        category: category_from_str(&row.get::<String>(2)?),
                        weight: row.get(3)?,
                        score: 1.0 - distance,
                        created_at: row.get(4)?,
                        retrieval_count: row.get(5)?,
                    },
                );
            }
            drain_rows(&mut rows).await?;
        }

        // Keyword path (Tantivy). Only surfaces hits vector search missed; the
        // FTS index is auto-maintained on insert/update/delete.
        if migrations::fts_available(conn).await.unwrap_or(false) {
            let fts_q = sanitize_query(query);
            if !fts_q.is_empty() {
                let cand = (top_k * 4).max(20) as usize;
                let sql = "SELECT * FROM memories WHERE fts_match(content, ?1)";
                if let Ok(mut rows) = conn.query(sql, params![fts_q.as_str()]).await {
                    // FTS hits in BM25 order; `SELECT *` is required by the
                    // Turso FTS rewrite, so read by `memories` column index.
                    let mut hits: Vec<(String, String, String, f64, i64, u32)> = Vec::new();
                    let mut i = 0usize;
                    while let Some(row) = rows.next().await? {
                        i += 1;
                        if i > cand {
                            break;
                        }
                        hits.push((
                            row.get(0)?, // id
                            row.get(1)?, // content
                            row.get(3)?, // category
                            row.get(4)?, // weight
                            row.get(6)?, // created_at
                            row.get(8)?, // retrieval_count
                        ));
                    }
                    drain_rows(&mut rows).await?;

                    // Cosine for FTS-only ids that do have embeddings.
                    let missing: Vec<String> = hits
                        .iter()
                        .filter(|(id, ..)| !by_id.contains_key(id))
                        .map(|(id, ..)| id.clone())
                        .collect();
                    let cosines = keyword_cosines(conn, &missing, emb_buf, vfn).await?;

                    for (rank, (id, content, category, weight, created_at, retrieval_count)) in
                        hits.into_iter().enumerate()
                    {
                        if by_id.contains_key(&id) {
                            continue;
                        }
                        let score = cosines.get(&id).copied().unwrap_or_else(|| keyword_score(rank));
                        by_id.insert(
                            id.clone(),
                            Memory {
                                id,
                                content,
                                category: category_from_str(&category),
                                weight,
                                score,
                                created_at,
                                retrieval_count,
                            },
                        );
                    }
                }
            }
        }

        let mut out: Vec<Memory> = by_id.into_values().collect();
        out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(top_k as usize);
        Ok(out)
    }
}
