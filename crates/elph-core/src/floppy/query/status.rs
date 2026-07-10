use anyhow::{Context, Result};

use super::super::store::MemoryStore;
use super::super::types::{CategoryCount, StoreStatus, TopMemory};
use super::super::util::{category_from_str, drain_rows};

impl MemoryStore {
    /// Extended store statistics (counts, categories, top memories, task metrics).
    pub async fn get_status(&self) -> Result<StoreStatus> {
        self.init().await?;
        self.with_db(|conn| async move {
            let (total_memories, completed_tasks, total_tasks, avg_score) = {
                let mut rows = conn
                    .query(
                        "SELECT
                            (SELECT COUNT(*) FROM memories),
                            (SELECT COUNT(*) FROM tasks WHERE finished_at IS NOT NULL),
                            (SELECT COUNT(*) FROM tasks),
                            (SELECT AVG(task_score) FROM tasks WHERE task_score IS NOT NULL)",
                        (),
                    )
                    .await?;
                let row = rows.next().await?.context("no status row")?;
                let stats = (
                    row.get::<i64>(0)?,
                    row.get::<i64>(1)?,
                    row.get::<i64>(2)?,
                    row.get::<Option<f64>>(3)?.unwrap_or(0.0),
                );
                drain_rows(&mut rows).await?;
                stats
            };

            let mut cat_rows = conn
                .query(
                    "SELECT category, COUNT(*) as c FROM memories GROUP BY category ORDER BY c DESC",
                    (),
                )
                .await?;
            let mut categories = Vec::new();
            while let Some(row) = cat_rows.next().await? {
                categories.push(CategoryCount {
                    category: category_from_str(&row.get::<String>(0)?),
                    count: row.get::<i64>(1)? as u32,
                });
            }
            drain_rows(&mut cat_rows).await?;

            let mut top_rows = conn
                .query(
                    "SELECT content, weight, retrieval_count FROM memories ORDER BY weight DESC LIMIT 5",
                    (),
                )
                .await?;
            let mut top_memories = Vec::new();
            while let Some(row) = top_rows.next().await? {
                top_memories.push(TopMemory {
                    content: row.get(0)?,
                    weight: row.get(1)?,
                    retrieval_count: row.get(2)?,
                });
            }
            drain_rows(&mut top_rows).await?;

            Ok(StoreStatus {
                total_memories: total_memories as u32,
                completed_tasks: completed_tasks as u32,
                total_tasks: total_tasks as u32,
                avg_task_score: avg_score,
                categories,
                top_memories,
            })
        })
        .await
    }
}
