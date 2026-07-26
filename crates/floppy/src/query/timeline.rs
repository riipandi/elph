use anyhow::Result;
use turso::params;

use super::super::store::MemoryStore;
use super::super::types::{TimelineEvent, TimelineEventKind};
use super::super::util::drain_rows;

impl MemoryStore {
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
                    format!("{}...", &desc[..80])
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
                    format!("{}...", &content[..80])
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
}
