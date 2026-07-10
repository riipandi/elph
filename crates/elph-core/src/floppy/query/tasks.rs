use std::collections::HashMap;

use anyhow::Result;
use turso::params;

use super::super::store::MemoryStore;
use super::super::types::{TaskCreatedMemory, TaskRecord, TaskRetrieval, TaskStatus};
use super::super::util::{category_from_str, drain_rows};

impl MemoryStore {
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
}
