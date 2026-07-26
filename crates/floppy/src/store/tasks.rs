use anyhow::Result;
use anyhow::bail;
use turso::params;

use super::{MemoryStore, SelfReportRow, WeightUpdate};
use super::{batch_set_weights, fetch_weights, new_id, now_secs, touch_retrieved_memories};
use crate::scoring::{compute_credit, compute_task_score, initial_weight, update_baseline, update_weight};
use crate::types::{Memory, MemoryCategory, ReportCorrectionInput, ReportUserInput, StartTaskResult, TaskEndInput};
use crate::util::{category_from_str, drain_rows, vec_buf};

impl MemoryStore {
    pub async fn start_task(&self, description: &str) -> Result<StartTaskResult> {
        self.init().await?;
        let task_id = new_id();
        let now = now_secs();

        // Embed outside with_db — no lock held during model inference
        let task_embedding = (self.embed)(description).await?;
        self.embed_pending().await?;

        let decay_rate = self.decay_rate;
        let top_k = self.top_k;
        let emb_buf = vec_buf(&task_embedding);
        let retrieval_sql = self.retrieval_sql();
        let task_id_clone = task_id.clone();
        let description = description.to_string();

        let memories = self
            .with_db(move |conn| async move {
                conn.execute(
                    "INSERT INTO tasks (id, description, embedding, started_at) VALUES (?, ?, ?, ?)",
                    params![task_id_clone.as_str(), description.as_str(), emb_buf.as_slice(), now],
                )
                .await?;

                let mut rows = conn
                    .query(
                        retrieval_sql.as_ref(),
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

                for mem in &mems {
                    conn.execute(
                        "INSERT OR IGNORE INTO memory_retrievals (memory_id, task_id, similarity) VALUES (?, ?, ?)",
                        params![mem.id.as_str(), task_id_clone.as_str(), mem.score],
                    )
                    .await?;
                }

                let memory_ids: Vec<String> = mems.iter().map(|m| m.id.clone()).collect();
                touch_retrieved_memories(&conn, &memory_ids, now).await?;

                Ok(mems)
            })
            .await?;

        *self.current_task_id.lock().unwrap() = Some(task_id.clone());
        Ok(StartTaskResult { task_id, memories })
    }

    pub async fn report_correction(&self, input: ReportCorrectionInput) -> Result<String> {
        self.init().await?;
        let id = new_id();
        let now = now_secs();
        let tokens_wasted = input.tokens_wasted;
        let _tools_wasted = input.tools_wasted;

        let content = format!(
            "{}\n\nFailed approach: {}\nWorking approach: {}",
            input.lesson, input.what_failed, input.what_worked
        );
        let embedding = (self.embed)(&content).await?;
        let emb_buf = vec_buf(&embedding);
        let current_task = self.current_task_id.lock().unwrap().clone();

        // AVG query in its own connection — mixing read query + write in one Turso
        // session can leave the INSERT uncommitted when the connection drops.
        let avg_tokens = self
            .with_db(|conn| async move {
                let mut rows = conn
                    .query("SELECT AVG(tokens_used) as avg FROM tasks WHERE tokens_used IS NOT NULL", ())
                    .await?;
                let avg = match rows.next().await? {
                    Some(row) => row.get::<Option<f64>>(0)?.unwrap_or(10_000.0),
                    None => 10_000.0,
                };
                drain_rows(&mut rows).await?;
                Ok(avg)
            })
            .await?;

        let weight = initial_weight(
            MemoryCategory::Correction,
            None,
            tokens_wasted.map(|t| t as f64),
            Some(avg_tokens),
        );
        self.with_db(move |conn| async move {
            let changes = conn
                .execute(
                    "INSERT INTO memories (id, content, embedding, category, weight, initial_cost, created_at, source_task) VALUES (?, ?, ?, 'correction', ?, ?, ?, ?)",
                    params![id.clone(), content, emb_buf, weight, tokens_wasted.unwrap_or(0), now, current_task],
                )
                .await?;
            if changes == 0 {
                bail!("report_correction: INSERT affected 0 rows");
            }
            Ok(id)
        })
        .await
    }

    pub async fn report_user_input(&self, input: ReportUserInput) -> Result<String> {
        self.init().await?;
        let id = new_id();
        let now = now_secs();

        let embedding = (self.embed)(&input.lesson).await?;
        let emb_buf = vec_buf(&embedding);
        let weight = initial_weight(MemoryCategory::User, Some(input.source), None, None);
        let current_task = self.current_task_id.lock().unwrap().clone();

        self.with_db(move |conn| async move {
            conn.execute(
                "INSERT INTO memories (id, content, embedding, category, weight, created_at, source_task) VALUES (?, ?, ?, 'user', ?, ?, ?)",
                params![id.clone(), input.lesson, emb_buf, weight, now, current_task],
            )
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn end_task(&self, task_id: &str, input: TaskEndInput) -> Result<()> {
        self.init().await?;
        let now = now_secs();

        let baseline_snapshot = *self.baseline.lock().unwrap();
        let task_score = compute_task_score(
            &baseline_snapshot,
            input.tokens_used as f64,
            input.errors as f64,
            input.user_corrections as f64,
            input.completed,
        );
        let new_baseline = update_baseline(
            &baseline_snapshot,
            input.tokens_used as f64,
            input.errors as f64,
            input.user_corrections as f64,
        );
        *self.baseline.lock().unwrap() = new_baseline;

        let learning_rate = self.learning_rate;
        let task_id_owned = task_id.to_string();
        let task_id_check = task_id_owned.clone();
        let baseline_json = serde_json::to_string(&new_baseline)?;

        // Pre-fetch weights in a separate connection — read+write in one Turso session
        // can prevent weight UPDATEs from persisting (same issue as report_correction).
        let (weight_updates, self_report_entries): (Vec<WeightUpdate>, Vec<SelfReportRow>) =
            if let Some(ref self_report) = input.self_report {
                if self_report.is_empty() {
                    (Vec::new(), Vec::new())
                } else {
                    let num_retrieved = self_report.len() as u32;
                    let ids: Vec<String> = self_report.iter().map(|e| e.memory_id.clone()).collect();
                    let weights = self
                        .with_db(|conn| async move { fetch_weights(&conn, &ids).await })
                        .await?;

                    let mut weight_updates = Vec::with_capacity(self_report.len());
                    let mut self_report_entries = Vec::with_capacity(self_report.len());
                    for entry in self_report {
                        let credit = compute_credit(task_score, entry.score as f64, num_retrieved);
                        self_report_entries.push((entry.memory_id.clone(), entry.score, credit));
                        if let Some(old) = weights.get(&entry.memory_id) {
                            weight_updates.push((entry.memory_id.clone(), update_weight(*old, credit, learning_rate)));
                        }
                    }
                    (weight_updates, self_report_entries)
                }
            } else {
                (Vec::new(), Vec::new())
            };

        self.with_db(move |conn| async move {
            conn.execute(
                r#"
                UPDATE tasks SET
                  tokens_used = ?, tool_calls = ?, errors = ?,
                  user_corrections = ?, completed = ?, task_score = ?, finished_at = ?
                WHERE id = ?
                "#,
                params![
                    input.tokens_used,
                    input.tool_calls,
                    input.errors,
                    input.user_corrections,
                    input.completed as i64,
                    task_score,
                    now,
                    task_id_owned.clone(),
                ],
            )
            .await?;

            conn.execute(
                "INSERT INTO meta (key, value) VALUES ('baseline', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![baseline_json],
            )
            .await?;

            batch_set_weights(&conn, &weight_updates).await?;

            for (memory_id, score, credit) in &self_report_entries {
                conn.execute(
                    "UPDATE memory_retrievals SET self_report = ?, credit = ? WHERE memory_id = ? AND task_id = ?",
                    params![*score as f64, credit, memory_id.clone(), task_id_owned.clone()],
                )
                .await?;
            }

            Ok(())
        })
        .await?;

        let mut cur = self.current_task_id.lock().unwrap();
        if cur.as_deref() == Some(task_id_check.as_str()) {
            *cur = None;
        }
        Ok(())
    }
}
