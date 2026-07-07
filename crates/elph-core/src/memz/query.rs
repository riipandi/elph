use anyhow::{Context, Result};
use turso::params;

use super::store::MemoryStore;
use super::types::{
    CategoryCount, Memory, MemoryCategory, MemoryRecord, StoreStatus, TaskCreatedMemory, TaskRecord, TaskRetrieval,
    TaskStatus, TimelineEvent, TimelineEventKind, TopMemory,
};
use super::util::{category_from_str, embedding_status, retrieval_sql, vec_buf};

impl MemoryStore {
    /// Extended store status (`elph memory status`).
    pub async fn get_status(&self) -> Result<StoreStatus> {
        self.init().await?;
        self.with_db(|conn| async move {
            let total_memories: i64 = {
                let mut rows = conn.query("SELECT COUNT(*) FROM memories", ()).await?;
                rows.next().await?.context("no row")?.get(0)?
            };
            let completed_tasks: i64 = {
                let mut rows = conn
                    .query("SELECT COUNT(*) FROM tasks WHERE finished_at IS NOT NULL", ())
                    .await?;
                rows.next().await?.context("no row")?.get(0)?
            };
            let total_tasks: i64 = {
                let mut rows = conn.query("SELECT COUNT(*) FROM tasks", ()).await?;
                rows.next().await?.context("no row")?.get(0)?
            };
            let avg_score: f64 = {
                let mut rows = conn
                    .query("SELECT AVG(task_score) FROM tasks WHERE task_score IS NOT NULL", ())
                    .await?;
                match rows.next().await? {
                    Some(row) => row.get::<Option<f64>>(0)?.unwrap_or(0.0),
                    None => 0.0,
                }
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

    /// List memories, optionally filtered by category (`elph memory list`).
    pub async fn list_memories(&self, category: Option<MemoryCategory>) -> Result<Vec<MemoryRecord>> {
        self.init().await?;
        let filter = category.map(super::util::category_str);
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
                conn.query(&sql, params![params[0].clone()]).await?
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
                    embedding_status: embedding_status(row.get::<Option<i64>>(6)?),
                });
            }
            Ok(out)
        })
        .await
    }

    /// List recent tasks with retrievals and memories created during each task (`elph memory tasks`).
    pub async fn list_tasks(&self, limit: u32) -> Result<Vec<TaskRecord>> {
        self.init().await?;
        self.with_db(move |conn| async move {
            let mut rows = conn
                .query(
                    r#"
                    SELECT id, description, tokens_used, tool_calls, errors, user_corrections,
                           completed, task_score, started_at, finished_at
                    FROM tasks
                    ORDER BY started_at DESC
                    LIMIT ?
                    "#,
                    params![limit],
                )
                .await?;

            let mut tasks = Vec::new();
            while let Some(row) = rows.next().await? {
                let id: String = row.get(0)?;
                let finished_at: Option<i64> = row.get(9)?;
                let completed: Option<i64> = row.get(6)?;
                let status = match finished_at {
                    None => TaskStatus::InProgress,
                    Some(_) if completed == Some(1) => TaskStatus::Completed,
                    Some(_) => TaskStatus::Failed,
                };

                let mut ret_rows = conn
                    .query(
                        r#"
                        SELECT r.memory_id, r.similarity, r.self_report, r.credit,
                               substr(m.content, 1, 80) as preview, m.category
                        FROM memory_retrievals r
                        JOIN memories m ON r.memory_id = m.id
                        WHERE r.task_id = ?
                        "#,
                        params![id.clone()],
                    )
                    .await?;
                let mut retrievals = Vec::new();
                while let Some(r) = ret_rows.next().await? {
                    let self_report: Option<f64> = r.get(2)?;
                    retrievals.push(TaskRetrieval {
                        memory_id: r.get(0)?,
                        similarity: r.get::<Option<f64>>(1)?,
                        self_report: self_report.map(|s| s.round() as u8),
                        credit: r.get(3)?,
                        preview: r.get(4)?,
                        category: category_from_str(&r.get::<String>(5)?),
                    });
                }

                let mut created_rows = conn
                    .query(
                        "SELECT category, substr(content, 1, 60) as preview FROM memories WHERE source_task = ?",
                        params![id.clone()],
                    )
                    .await?;
                let mut created_memories = Vec::new();
                while let Some(c) = created_rows.next().await? {
                    created_memories.push(TaskCreatedMemory {
                        category: category_from_str(&c.get::<String>(0)?),
                        preview: c.get(1)?,
                    });
                }

                tasks.push(TaskRecord {
                    id,
                    description: row.get(1)?,
                    tokens_used: row.get::<Option<i64>>(2)?.map(|n| n as u32),
                    tool_calls: row.get::<Option<i64>>(3)?.map(|n| n as u32),
                    errors: row.get::<Option<i64>>(4)?.map(|n| n as u32),
                    user_corrections: row.get::<Option<i64>>(5)?.map(|n| n as u32),
                    status,
                    task_score: row.get(7)?,
                    started_at: row.get(8)?,
                    finished_at,
                    retrievals,
                    created_memories,
                });
            }
            Ok(tasks)
        })
        .await
    }

    /// Merged timeline of tasks and memory events (`elph memory log`).
    pub async fn get_timeline(&self, limit: u32) -> Result<Vec<TimelineEvent>> {
        self.init().await?;
        self.with_db(move |conn| async move {
            let mut events = Vec::new();

            let mut task_rows = conn
                .query(
                    r#"
                    SELECT description, task_score, tokens_used, errors, completed, started_at
                    FROM tasks ORDER BY started_at DESC LIMIT ?
                    "#,
                    params![limit],
                )
                .await?;
            while let Some(row) = task_rows.next().await? {
                let started_at: i64 = row.get(5)?;
                let completed: Option<i64> = row.get(4)?;
                let status = if completed == Some(1) { "OK" } else { "FAIL" };
                let score = row
                    .get::<Option<f64>>(1)?
                    .map(|s| format!("{s:.2}"))
                    .unwrap_or_else(|| "?".into());
                let tokens = row
                    .get::<Option<i64>>(2)?
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".into());
                let errors = row.get::<Option<i64>>(3)?.unwrap_or(0);
                let desc: String = row.get::<Option<String>>(0)?.unwrap_or_default();
                let desc = if desc.len() > 80 {
                    format!("{}…", &desc[..80])
                } else {
                    desc
                };
                events.push(TimelineEvent {
                    timestamp: started_at,
                    kind: TimelineEventKind::Task,
                    summary: format!("TASK [{status}] score={score} {tokens}tok {errors}err — {desc}"),
                });
            }

            let mut mem_rows = conn
                .query(
                    "SELECT content, category, weight, created_at FROM memories ORDER BY created_at DESC LIMIT ?",
                    params![limit],
                )
                .await?;
            while let Some(row) = mem_rows.next().await? {
                let created_at: i64 = row.get(3)?;
                let category: String = row.get(1)?;
                let weight: f64 = row.get(2)?;
                let content: String = row.get(0)?;
                let preview = if content.len() > 80 {
                    format!("{}…", &content[..80])
                } else {
                    content
                };
                events.push(TimelineEvent {
                    timestamp: created_at,
                    kind: TimelineEventKind::Memory,
                    summary: format!("MEM  [{category}] w={weight:.2} — {preview}"),
                });
            }

            events.sort_by_key(|e| e.timestamp);
            Ok(events)
        })
        .await
    }

    /// Read-only semantic search — no task record, no retrieval side effects.
    pub async fn search_memories(&self, query: &str) -> Result<Vec<Memory>> {
        self.init().await?;
        let embedding = (self.embed_fn())(query).await?;
        let emb_buf = vec_buf(&embedding);
        let vfn = self.vector_fn();
        let sql = retrieval_sql(vfn);
        let decay_rate = self.decay_rate();
        let top_k = self.top_k();
        let now = super::store::now_secs();

        self.with_db(move |conn| async move {
            let mut rows = conn
                .query(&sql, params![emb_buf.clone(), emb_buf, decay_rate, now, top_k])
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
            Ok(mems)
        })
        .await
    }

    /// Semantic search via full task lifecycle (`elph memory search` — creates a task).
    pub async fn search(&self, query: &str) -> Result<super::types::StartTaskResult> {
        let _ = self.embed_pending().await?;
        self.start_task(query).await
    }
}
