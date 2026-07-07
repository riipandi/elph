use std::collections::HashMap;

use anyhow::{Context, Result};
use turso::params;

use super::store::MemoryStore;
use super::types::{
    CategoryCount, Memory, MemoryCategory, MemoryRecord, StoreStatus, TaskCreatedMemory, TaskRecord, TaskRetrieval,
    TaskStatus, TimelineEvent, TimelineEventKind, TopMemory,
};
use super::util::{category_from_str, drain_rows, embedding_status, vec_buf};

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

    /// List memories, optionally filtered by category.
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

    /// List recent tasks with retrievals and memories created during each task.
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

            struct TaskRow {
                id: String,
                description: Option<String>,
                tokens_used: Option<i64>,
                tool_calls: Option<i64>,
                errors: Option<i64>,
                user_corrections: Option<i64>,
                completed: Option<i64>,
                task_score: Option<f64>,
                started_at: Option<i64>,
                finished_at: Option<i64>,
            }

            let mut task_rows = Vec::new();
            let mut task_ids = Vec::new();
            while let Some(row) = rows.next().await? {
                let id: String = row.get(0)?;
                task_ids.push(id.clone());
                task_rows.push(TaskRow {
                    id,
                    description: row.get(1)?,
                    tokens_used: row.get(2)?,
                    tool_calls: row.get(3)?,
                    errors: row.get(4)?,
                    user_corrections: row.get(5)?,
                    completed: row.get(6)?,
                    task_score: row.get(7)?,
                    started_at: row.get(8)?,
                    finished_at: row.get(9)?,
                });
            }
            drain_rows(&mut rows).await?;

            let mut retrievals_by_task: HashMap<String, Vec<TaskRetrieval>> = HashMap::new();
            let mut created_by_task: HashMap<String, Vec<TaskCreatedMemory>> = HashMap::new();

            if !task_ids.is_empty() {
                let placeholders = std::iter::repeat_n("?", task_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");

                let retrieval_sql = format!(
                    r#"
                    SELECT r.task_id, r.memory_id, r.similarity, r.self_report, r.credit,
                           substr(m.content, 1, 80) as preview, m.category
                    FROM memory_retrievals r
                    JOIN memories m ON r.memory_id = m.id
                    WHERE r.task_id IN ({placeholders})
                    "#
                );
                let mut ret_rows = conn
                    .query(&retrieval_sql, turso::params_from_iter(task_ids.iter().map(String::as_str)))
                    .await?;
                while let Some(r) = ret_rows.next().await? {
                    let task_id: String = r.get(0)?;
                    let self_report: Option<f64> = r.get(3)?;
                    retrievals_by_task
                        .entry(task_id)
                        .or_default()
                        .push(TaskRetrieval {
                            memory_id: r.get(1)?,
                            similarity: r.get::<Option<f64>>(2)?,
                            self_report: self_report.map(|s| s.round() as u8),
                            credit: r.get(4)?,
                            preview: r.get(5)?,
                            category: category_from_str(&r.get::<String>(6)?),
                        });
                }
                drain_rows(&mut ret_rows).await?;

                let created_sql = format!(
                    "SELECT source_task, category, substr(content, 1, 60) as preview FROM memories WHERE source_task IN ({placeholders})"
                );
                let mut created_rows = conn
                    .query(&created_sql, turso::params_from_iter(task_ids.iter().map(String::as_str)))
                    .await?;
                while let Some(c) = created_rows.next().await? {
                    let task_id: String = c.get(0)?;
                    created_by_task.entry(task_id).or_default().push(TaskCreatedMemory {
                        category: category_from_str(&c.get::<String>(1)?),
                        preview: c.get(2)?,
                    });
                }
                drain_rows(&mut created_rows).await?;
            }

            let mut tasks = Vec::with_capacity(task_rows.len());
            for row in task_rows {
                let status = match row.finished_at {
                    None => TaskStatus::InProgress,
                    Some(_) if row.completed == Some(1) => TaskStatus::Completed,
                    Some(_) => TaskStatus::Failed,
                };

                tasks.push(TaskRecord {
                    id: row.id.clone(),
                    description: row.description,
                    tokens_used: row.tokens_used.map(|n| n as u32),
                    tool_calls: row.tool_calls.map(|n| n as u32),
                    errors: row.errors.map(|n| n as u32),
                    user_corrections: row.user_corrections.map(|n| n as u32),
                    status,
                    task_score: row.task_score,
                    started_at: row.started_at,
                    finished_at: row.finished_at,
                    retrievals: retrievals_by_task.remove(&row.id).unwrap_or_default(),
                    created_memories: created_by_task.remove(&row.id).unwrap_or_default(),
                });
            }
            Ok(tasks)
        })
        .await
    }

    /// Merged timeline of tasks and memory events.
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
            drain_rows(&mut task_rows).await?;

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
            drain_rows(&mut mem_rows).await?;

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
        let sql = self.retrieval_sql();
        let decay_rate = self.decay_rate();
        let top_k = self.top_k();
        let now = super::store::now_secs();

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

    /// Semantic search via full task lifecycle (creates a task record).
    pub async fn search(&self, query: &str) -> Result<super::types::StartTaskResult> {
        self.start_task(query).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::memz::types::{MemoryCategory, MemoryReportInput, ReportCorrectionInput, TaskEndInput, UserInputSource};
    use std::sync::Arc;

    fn mock_embed() -> EmbedFn {
        Arc::new(|text: &str| {
            let text = text.to_string();
            Box::pin(async move {
                let mut vec = vec![0.0f32; 4];
                for (i, b) in text.bytes().enumerate() {
                    vec[i % 4] += b as f32;
                }
                let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for x in &mut vec {
                        *x /= norm;
                    }
                }
                Ok(vec)
            })
        })
    }

    struct TestCtx {
        _dir: tempfile::TempDir,
        store: MemoryStore,
    }

    impl TestCtx {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let db_path = dir.path().join("memory.db").to_string_lossy().into_owned();
            let store = create_memory_store(MemzConfig::new(db_path, "test").top_k(3).dimensions(4), mock_embed());
            Self { _dir: dir, store }
        }
    }

    #[tokio::test]
    async fn get_status_includes_categories() {
        let ctx = TestCtx::new();
        ctx.store
            .report_user_input(crate::memz::ReportUserInput {
                lesson: "use pnpm".into(),
                source: UserInputSource::UserInput,
            })
            .await
            .expect("report");

        let status = ctx.store.get_status().await.expect("status");
        assert_eq!(status.total_memories, 1);
        assert_eq!(status.categories.len(), 1);
        assert_eq!(status.categories[0].category, MemoryCategory::User);
    }

    #[tokio::test]
    async fn list_memories_filters_category() {
        let ctx = TestCtx::new();
        ctx.store
            .insert_raw_memory("insight note", MemoryCategory::Insight, 1.0)
            .await
            .expect("insight");
        ctx.store
            .report_user_input(crate::memz::ReportUserInput {
                lesson: "user note".into(),
                source: UserInputSource::UserInput,
            })
            .await
            .expect("user");

        let all = ctx.store.list_memories(None).await.expect("all");
        assert_eq!(all.len(), 2);

        let users = ctx
            .store
            .list_memories(Some(MemoryCategory::User))
            .await
            .expect("users");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].category, MemoryCategory::User);
    }

    #[tokio::test]
    async fn search_memories_is_read_only() {
        let ctx = TestCtx::new();
        let mem_id = ctx
            .store
            .report_user_input(crate::memz::ReportUserInput {
                lesson: "auth middleware path".into(),
                source: UserInputSource::UserCorrection,
            })
            .await
            .expect("report");

        let hits = ctx.store.search_memories("auth middleware").await.expect("search");
        assert!(hits.iter().any(|m| m.id == mem_id));

        let tasks = ctx.store.list_tasks(10).await.expect("tasks");
        assert!(tasks.is_empty(), "search_memories must not create tasks");
    }

    #[tokio::test]
    async fn report_unified_insight_and_end_with_decay() {
        let ctx = TestCtx::new();
        let id = ctx
            .store
            .report(MemoryReportInput::insight("VDBE architecture"))
            .await
            .expect("insight");
        assert!(!id.is_empty());

        let start = ctx.store.start_task("explore vm").await.expect("start");
        let result = ctx
            .store
            .end_task_with_decay(
                &start.task_id,
                TaskEndInput {
                    tokens_used: 100,
                    tool_calls: 1,
                    errors: 0,
                    user_corrections: 0,
                    completed: true,
                    self_report: None,
                },
            )
            .await
            .expect("end+decay");
        assert!(result.decay.decayed >= 1);

        let timeline = ctx.store.get_timeline(10).await.expect("timeline");
        assert!(!timeline.is_empty());
    }

    #[tokio::test]
    async fn contradict_wrapper_returns_struct() {
        let ctx = TestCtx::new();
        let id = ctx
            .store
            .report_correction(ReportCorrectionInput {
                lesson: "old".into(),
                what_failed: "a".into(),
                what_worked: "b".into(),
                tokens_wasted: None,
                tools_wasted: None,
            })
            .await
            .expect("report");

        let result = ctx.store.contradict(&id, Some("corrected")).await.expect("contradict");
        assert!(result.deleted);
        assert!(result.correction_id.is_some());
    }

    #[test]
    fn memz_paths_project_local() {
        let paths = MemzPaths::project_local();
        assert!(paths.db_path().ends_with(".memz/memory.db"));
        let cfg = paths.config("sess");
        assert_eq!(cfg.session_id, "sess");
    }
}
