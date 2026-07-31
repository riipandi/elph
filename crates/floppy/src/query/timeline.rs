use anyhow::Result;
use turso::params;

use super::super::store::MemoryStore;
use super::super::types::{TimelineEvent, TimelineEventKind};
use super::super::util::drain_rows;

fn preview_text(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let t: String = trimmed.chars().take(max_chars).collect();
        format!("{t}…")
    }
}

impl MemoryStore {
    /// Merged timeline of tasks and memory events (newest first).
    pub async fn get_timeline(&self, limit: u32) -> Result<Vec<TimelineEvent>> {
        self.init().await?;
        let limit = limit.max(1);
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
                let status = match completed {
                    Some(1) => "ok",
                    Some(0) => "failed",
                    _ => "open",
                };
                let score = row
                    .get::<Option<f64>>(1)?
                    .map(|s| format!("{s:.2}"))
                    .unwrap_or_else(|| "—".into());
                let tokens = row
                    .get::<Option<i64>>(2)?
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "—".into());
                let errors = row.get::<Option<i64>>(3)?.unwrap_or(0);
                let desc: String = row.get::<Option<String>>(0)?.unwrap_or_default();
                let desc = preview_text(&desc, 72);
                events.push(TimelineEvent {
                    timestamp: started_at,
                    kind: TimelineEventKind::Task,
                    summary: format!("[{status}] score={score} {tokens} tok, {errors} err — {desc}"),
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
                let preview = preview_text(&content, 72);
                events.push(TimelineEvent {
                    timestamp: created_at,
                    kind: TimelineEventKind::Memory,
                    summary: format!("[{category}] w={weight:.2} — {preview}"),
                });
            }
            drain_rows(&mut mem_rows).await?;

            // Newest first for "log" UX.
            events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            // Cap total merged list so output stays readable.
            events.truncate(limit as usize * 2);
            Ok(events)
        })
        .await
    }
}
