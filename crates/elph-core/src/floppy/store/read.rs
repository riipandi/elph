use anyhow::{Context, Result};
use turso::params;

use super::MemoryStore;
use crate::floppy::types::{Memory, MemoryStats, TopMemory};
use crate::floppy::util::{category_from_str, drain_rows};

impl MemoryStore {
    pub async fn get_stats(&self) -> Result<MemoryStats> {
        self.init().await?;
        self.with_db(|conn| async move {
            let (mem_count, task_count, avg_score) = {
                let mut rows = conn
                    .query(
                        "SELECT
                            (SELECT COUNT(*) FROM memories),
                            (SELECT COUNT(*) FROM tasks),
                            (SELECT AVG(task_score) FROM tasks WHERE task_score IS NOT NULL)",
                        (),
                    )
                    .await?;
                let counts = rows.next().await?.context("no stats row")?;
                let stats = (
                    counts.get::<i64>(0)?,
                    counts.get::<i64>(1)?,
                    counts.get::<Option<f64>>(2)?.unwrap_or(0.0),
                );
                drain_rows(&mut rows).await?;
                stats
            };

            let mut rows = conn
                .query(
                    "SELECT content, weight, retrieval_count FROM memories ORDER BY weight DESC LIMIT 10",
                    (),
                )
                .await?;
            let mut top_memories = Vec::new();
            while let Some(row) = rows.next().await? {
                top_memories.push(TopMemory {
                    content: row.get(0)?,
                    weight: row.get(1)?,
                    retrieval_count: row.get(2)?,
                });
            }
            drain_rows(&mut rows).await?;

            Ok(MemoryStats {
                total_memories: mem_count as u32,
                task_count: task_count as u32,
                avg_task_score: avg_score,
                top_memories,
            })
        })
        .await
    }

    // -------------------------------------------------------------------
    // Hook-oriented methods (no embedding model needed for get/insert)
    // -------------------------------------------------------------------

    pub async fn get_top_by_weight(&self, limit: u32) -> Result<Vec<Memory>> {
        self.init().await?;
        self.with_db(move |conn| async move {
            let mut rows = conn
                .query(
                    "SELECT id, content, category, weight, created_at, retrieval_count FROM memories ORDER BY weight DESC LIMIT ?",
                    params![limit],
                )
                .await?;

            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                let weight: f64 = row.get(3)?;
                out.push(Memory {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: category_from_str(&row.get::<String>(2)?),
                    weight,
                    score: weight,
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
