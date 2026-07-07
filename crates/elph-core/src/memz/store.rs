use anyhow::{Context, Result, bail};
use rand::RngExt;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use turso::{Builder, Connection, Database, params};
use uuid::Uuid;

use super::scoring::{
    compute_credit, compute_task_score, empty_baseline, initial_weight, update_baseline, update_weight,
};
use super::types::{
    DecayResult, Memory, MemoryCategory, MemoryStats, MemzConfig, ReportCorrectionInput, ReportUserInput,
    StartTaskResult, TaskBaseline, TaskEndInput, TopMemory, VectorType,
};

pub type EmbedFuture = Pin<Box<dyn Future<Output = Result<Vec<f32>>> + Send>>;
pub type EmbedFn = Arc<dyn Fn(&str) -> EmbedFuture + Send + Sync>;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS memories (
    id              TEXT PRIMARY KEY,
    content         TEXT NOT NULL,
    embedding       BLOB,
    category        TEXT NOT NULL,
    weight          REAL DEFAULT 1.0,
    initial_cost    INTEGER DEFAULT 0,
    created_at      INTEGER NOT NULL,
    last_retrieved  INTEGER,
    retrieval_count INTEGER DEFAULT 0,
    source_task     TEXT
);

CREATE TABLE IF NOT EXISTS tasks (
    id               TEXT PRIMARY KEY,
    description      TEXT,
    embedding        BLOB,
    tokens_used      INTEGER,
    tool_calls       INTEGER,
    errors           INTEGER,
    user_corrections INTEGER,
    completed        INTEGER,
    task_score       REAL,
    started_at       INTEGER,
    finished_at      INTEGER
);

CREATE TABLE IF NOT EXISTS memory_retrievals (
    memory_id   TEXT,
    task_id     TEXT,
    similarity  REAL,
    self_report REAL,
    credit      REAL,
    PRIMARY KEY (memory_id, task_id)
);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn new_id() -> String {
    Uuid::now_v7().to_string()
}

/// f32 vec -> raw LE bytes. Mirrors TS vecBuf: preserve float32 binary layout for the driver.
fn vec_buf(v: &[f32]) -> Vec<u8> {
    let byte_len = v.len() * std::mem::size_of::<f32>();
    let mut buf = Vec::with_capacity(byte_len);
    // SAFETY: f32 is plain-old-data; layout matches LE byte sequence the Turso driver expects.
    unsafe {
        buf.set_len(byte_len);
        std::ptr::copy_nonoverlapping(v.as_ptr().cast(), buf.as_mut_ptr(), byte_len);
    }
    buf
}

fn retrieval_sql(vfn: &str) -> String {
    format!(
        r#"
        SELECT
          id, content, category, weight, created_at, retrieval_count,
          vector_distance_cos({vfn}(embedding), {vfn}(?)) AS distance
        FROM memories
        WHERE embedding IS NOT NULL
        ORDER BY
          (1.0 - vector_distance_cos({vfn}(embedding), {vfn}(?)))
          * POWER(?, (CAST(? AS REAL) - COALESCE(last_retrieved, created_at)) / 86400.0)
        DESC
        LIMIT ?
        "#
    )
}

/// Remove retrieval rows whose memory was deleted (prevents unbounded table growth).
async fn delete_orphan_retrievals(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM memory_retrievals WHERE memory_id NOT IN (SELECT id FROM memories)",
        (),
    )
    .await?;
    Ok(())
}

async fn fetch_weights(conn: &Connection, ids: &[String]) -> Result<HashMap<String, f64>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(", ");
    let sql = format!("SELECT id, weight FROM memories WHERE id IN ({placeholders})");
    let mut rows = conn
        .query(&sql, turso::params_from_iter(ids.iter().map(String::as_str)))
        .await?;
    let mut out = HashMap::with_capacity(ids.len());
    while let Some(row) = rows.next().await? {
        out.insert(row.get::<String>(0)?, row.get::<f64>(1)?);
    }
    Ok(out)
}

fn category_str(c: MemoryCategory) -> &'static str {
    match c {
        MemoryCategory::Correction => "correction",
        MemoryCategory::Insight => "insight",
        MemoryCategory::User => "user",
        MemoryCategory::Consolidated => "consolidated",
        MemoryCategory::Discovery => "discovery",
    }
}

fn category_from_str(s: &str) -> MemoryCategory {
    match s {
        "correction" => MemoryCategory::Correction,
        "insight" => MemoryCategory::Insight,
        "user" => MemoryCategory::User,
        "consolidated" => MemoryCategory::Consolidated,
        _ => MemoryCategory::Discovery,
    }
}

pub struct MemoryStore {
    db_path: String,
    #[allow(dead_code)]
    session_id: String,
    embed: EmbedFn,
    vector_type: VectorType,
    retrieval_sql: OnceLock<String>,
    top_k: u32,
    learning_rate: f64,
    decay_rate: f64,

    initialized: Mutex<bool>,
    current_task_id: Mutex<Option<String>>,
    baseline: Mutex<TaskBaseline>,
}

impl MemoryStore {
    pub fn new(config: MemzConfig, embed: EmbedFn) -> Self {
        Self {
            db_path: config.db_path,
            session_id: config.session_id,
            embed,
            vector_type: config.vector_type.unwrap_or(VectorType::Vector32),
            retrieval_sql: OnceLock::new(),
            top_k: config.top_k.unwrap_or(5),
            learning_rate: config.learning_rate.unwrap_or(0.1),
            decay_rate: config.decay_rate.unwrap_or(0.995),
            initialized: Mutex::new(false),
            current_task_id: Mutex::new(None),
            baseline: Mutex::new(empty_baseline()),
        }
    }

    fn vfn(&self) -> &'static str {
        match self.vector_type {
            VectorType::Vector32 => "vector32",
            VectorType::Vector64 => "vector64",
            VectorType::Vector8 => "vector8",
            VectorType::Vector1 => "vector1",
        }
    }

    fn retrieval_sql(&self) -> &str {
        self.retrieval_sql.get_or_init(|| retrieval_sql(self.vfn()))
    }

    async fn open_db(&self) -> Result<Database> {
        const MAX_RETRIES: u32 = 10;
        const BASE_DELAY_MS: u64 = 50;

        let mut attempt = 0u32;
        loop {
            let build = Builder::new_local(&self.db_path)
                .experimental_multiprocess_wal(true)
                .build()
                .await;
            match build {
                Ok(db) => return Ok(db),
                Err(e) => {
                    if attempt >= MAX_RETRIES || !is_lock_err(&e.to_string()) {
                        return Err(e).context("build failed");
                    }
                }
            }
            let jitter: f64 = rand::rng().random();
            let delay = BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0);
            tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
            attempt += 1;
        }
    }

    /// Open short-lived conn, run fn, then drop both conn and db. Turso embedded driver
    /// locks the file at connect()-time; keep `Database` alive for the whole operation.
    /// Retry connect() w/ backoff if another process holds the lock.
    async fn with_db<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Connection) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        const MAX_RETRIES: u32 = 10;
        const BASE_DELAY_MS: u64 = 50;

        let db = self.open_db().await?;
        let conn = {
            let mut attempt = 0u32;
            loop {
                match db.connect() {
                    Ok(conn) => break conn,
                    Err(e) => {
                        if attempt >= MAX_RETRIES || !is_lock_err(&e.to_string()) {
                            return Err(e).context("connect failed");
                        }
                    }
                }
                let jitter: f64 = rand::rng().random();
                let delay = BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0);
                tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
                attempt += 1;
            }
        };

        conn.execute("PRAGMA busy_timeout = 5000", ()).await?;
        f(conn).await
    }

    pub async fn init(&self) -> Result<()> {
        if *self.initialized.lock().unwrap() {
            return Ok(());
        }
        self.with_db(|conn| async move {
            conn.execute_batch(SCHEMA).await?;

            // One-time migration: embeddings truncated by driver bug
            let changes = conn
                .execute(
                    "UPDATE memories SET embedding = NULL WHERE embedding IS NOT NULL AND length(embedding) < 1536",
                    (),
                )
                .await?;
            if changes > 0 {
                eprintln!("[memz] Fixed {changes} truncated embeddings (will re-embed on next start_task)");
            }

            // Load baseline
            let mut rows = conn.query("SELECT value FROM meta WHERE key = 'baseline'", ()).await?;
            if let Some(row) = rows.next().await? {
                let raw: String = row.get(0)?;
                return Ok(Some(raw));
            }
            Ok(None)
        })
        .await
        .map(|maybe_raw: Option<String>| {
            if let Some(raw) = maybe_raw {
                if let Ok(b) = serde_json::from_str::<TaskBaseline>(&raw) {
                    *self.baseline.lock().unwrap() = b;
                }
            }
        })?;

        *self.initialized.lock().unwrap() = true;
        Ok(())
    }

    // -------------------------------------------------------------------
    // Task lifecycle
    // -------------------------------------------------------------------

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
        let retrieval_sql = self.retrieval_sql().to_string();
        let task_id_clone = task_id.clone();
        let description = description.to_string();

        let memories = self
            .with_db(move |conn| {
                let emb_buf = emb_buf.clone();
                async move {
                    conn.execute(
                        "INSERT INTO tasks (id, description, embedding, started_at) VALUES (?, ?, ?, ?)",
                        params![task_id_clone.clone(), description, emb_buf.clone(), now],
                    )
                    .await?;

                    let mut rows = conn
                        .query(
                            &retrieval_sql,
                            params![emb_buf.clone(), emb_buf, decay_rate, now, top_k],
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

                    for mem in &mems {
                        conn.execute(
                            "INSERT OR IGNORE INTO memory_retrievals (memory_id, task_id, similarity) VALUES (?, ?, ?)",
                            params![mem.id.clone(), task_id_clone.clone(), mem.score],
                        )
                        .await?;

                        conn.execute(
                            "UPDATE memories SET last_retrieved = ?, retrieval_count = retrieval_count + 1 WHERE id = ?",
                            params![now, mem.id.clone()],
                        )
                        .await?;
                    }

                    Ok(mems)
                }
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
                    .query(
                        "SELECT AVG(tokens_used) as avg FROM tasks WHERE tokens_used IS NOT NULL",
                        (),
                    )
                    .await?;
                Ok(match rows.next().await? {
                    Some(row) => row.get::<Option<f64>>(0)?.unwrap_or(10_000.0),
                    None => 10_000.0,
                })
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

        let baseline_snapshot = self.baseline.lock().unwrap().clone();
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
        *self.baseline.lock().unwrap() = new_baseline.clone();

        let learning_rate = self.learning_rate;
        let task_id_owned = task_id.to_string();
        let task_id_check = task_id_owned.clone();
        let baseline_json = serde_json::to_string(&new_baseline)?;

        // Pre-fetch weights in a separate connection — read+write in one Turso session
        // can prevent weight UPDATEs from persisting (same issue as report_correction).
        let (weight_updates, self_report_entries): (Vec<(String, f64)>, Vec<(String, u8, f64)>) =
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

            for (memory_id, new_weight) in &weight_updates {
                conn.execute(
                    "UPDATE memories SET weight = ? WHERE id = ?",
                    params![new_weight, memory_id.clone()],
                )
                .await?;
            }

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

    // -------------------------------------------------------------------
    // Maintenance
    // -------------------------------------------------------------------

    pub async fn decay(&self) -> Result<DecayResult> {
        self.init().await?;
        let decay_rate = self.decay_rate;
        self.with_db(move |conn| async move {
            let decayed = conn
                .execute("UPDATE memories SET weight = weight * ?", params![decay_rate])
                .await?;
            let deleted = conn
                .execute("DELETE FROM memories WHERE weight < 0.15 AND retrieval_count > 5", ())
                .await?;
            delete_orphan_retrievals(&conn).await?;
            Ok(DecayResult {
                decayed: decayed as u32,
                deleted: deleted as u32,
            })
        })
        .await
    }

    pub async fn purge(&self, threshold: f64) -> Result<u32> {
        self.init().await?;
        self.with_db(move |conn| async move {
            let n = conn
                .execute("DELETE FROM memories WHERE weight < ?", params![threshold])
                .await?;
            delete_orphan_retrievals(&conn).await?;
            Ok(n as u32)
        })
        .await
    }

    pub async fn get_stats(&self) -> Result<MemoryStats> {
        self.init().await?;
        self.with_db(|conn| async move {
            let mem_count: i64 = {
                let mut rows = conn.query("SELECT COUNT(*) FROM memories", ()).await?;
                rows.next().await?.context("no row")?.get(0)?
            };
            let task_count: i64 = {
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
            Ok(out)
        })
        .await
    }

    pub async fn insert_raw_memory(&self, content: &str, category: MemoryCategory, weight: f64) -> Result<String> {
        self.init().await?;
        let id = new_id();
        let now = now_secs();
        let content = content.to_string();
        let cat = category_str(category).to_string();
        let current_task = self.current_task_id.lock().unwrap().clone();

        self.with_db(move |conn| async move {
            conn.execute(
                "INSERT INTO memories (id, content, embedding, category, weight, created_at, source_task) VALUES (?, ?, NULL, ?, ?, ?, ?)",
                params![id.clone(), content, cat, weight, now, current_task],
            )
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn embed_pending(&self) -> Result<usize> {
        self.init().await?;

        let rows: Vec<(String, String)> = self
            .with_db(|conn| async move {
                let mut r = conn
                    .query("SELECT id, content FROM memories WHERE embedding IS NULL", ())
                    .await?;
                let mut out = Vec::new();
                while let Some(row) = r.next().await? {
                    out.push((row.get::<String>(0)?, row.get::<String>(1)?));
                }
                Ok(out)
            })
            .await?;

        if rows.is_empty() {
            return Ok(0);
        }

        let mut embedded = Vec::with_capacity(rows.len());
        for (id, content) in &rows {
            let vec = (self.embed)(content).await?;
            embedded.push((id.clone(), vec_buf(&vec)));
        }

        let n = rows.len();
        self.with_db(move |conn| async move {
            for (id, emb) in embedded {
                conn.execute("UPDATE memories SET embedding = ? WHERE id = ?", params![emb, id])
                    .await?;
            }
            Ok(())
        })
        .await?;

        Ok(n)
    }

    pub async fn contradict_memory(&self, memory_id: &str, correction: Option<&str>) -> Result<(bool, Option<String>)> {
        self.init().await?;
        let mid = memory_id.to_string();

        let deleted = self
            .with_db(move |conn| async move {
                let changes = conn
                    .execute("DELETE FROM memories WHERE id = ?", params![mid.clone()])
                    .await?;
                conn.execute(
                    "DELETE FROM memory_retrievals WHERE memory_id = ?",
                    params![mid.clone()],
                )
                .await?;
                Ok(changes > 0)
            })
            .await?;

        let mut correction_id = None;
        if let (Some(correction), true) = (correction, deleted) {
            let embedding = (self.embed)(correction).await?;
            let emb_buf = vec_buf(&embedding);
            let id = new_id();
            let now = now_secs();
            let correction = correction.to_string();
            let current_task = self.current_task_id.lock().unwrap().clone();
            let id_clone = id.clone();

            self.with_db(move |conn| async move {
                conn.execute(
                    "INSERT INTO memories (id, content, embedding, category, weight, created_at, source_task) VALUES (?, ?, ?, 'correction', 2.0, ?, ?)",
                    params![id_clone, correction, emb_buf, now, current_task],
                )
                .await?;
                Ok(())
            })
            .await?;

            correction_id = Some(id);
        }

        Ok((deleted, correction_id))
    }

    pub async fn penalize_memory(&self, memory_id: &str, factor: f64) -> Result<()> {
        self.init().await?;
        let mid = memory_id.to_string();
        self.with_db(move |conn| async move {
            conn.execute(
                "UPDATE memories SET weight = MAX(weight * ?, 0.1) WHERE id = ?",
                params![factor, mid],
            )
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn close(&self) -> Result<()> {
        // No persistent conn — with_db opens/closes per op.
        *self.initialized.lock().unwrap() = false;
        Ok(())
    }
}

fn is_lock_err(msg: &str) -> bool {
    msg.contains("locked") || msg.contains("Locking")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memz::create_memory_store;
    use crate::memz::types::{
        MemzConfig, ReportCorrectionInput, ReportUserInput, SelfReportEntry, TaskEndInput, UserInputSource,
    };
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

    fn test_config(db_path: &str) -> MemzConfig {
        MemzConfig {
            db_path: db_path.to_string(),
            session_id: "test-session".to_string(),
            vector_type: None,
            dimensions: Some(4),
            top_k: Some(3),
            learning_rate: Some(0.1),
            decay_rate: Some(0.995),
        }
    }

    /// Holds a `tempfile::TempDir` so the DB path stays valid for the whole test.
    struct TestCtx {
        _dir: tempfile::TempDir,
        db_path: String,
    }

    impl TestCtx {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let db_path = dir.path().join("memory.db").to_string_lossy().into_owned();
            Self { _dir: dir, db_path }
        }

        fn store(&self) -> MemoryStore {
            create_memory_store(test_config(&self.db_path), mock_embed())
        }

        fn store_with(&self, mut config: MemzConfig) -> MemoryStore {
            config.db_path = self.db_path.clone();
            create_memory_store(config, mock_embed())
        }
    }

    fn assert_uuid_v7(id: &str) {
        let uuid = Uuid::parse_str(id).expect("valid uuid");
        assert_eq!(uuid.get_version(), Some(uuid::Version::SortRand));
    }

    #[tokio::test]
    async fn init_creates_schema() {
        let ctx = TestCtx::new();
        let store = ctx.store();
        store.init().await.expect("init");

        let stats = store.get_stats().await.expect("stats");
        assert_eq!(stats.total_memories, 0);
        assert_eq!(stats.task_count, 0);
    }

    #[tokio::test]
    async fn ids_use_uuid_v7() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let mem_id = store
            .report_user_input(ReportUserInput {
                lesson: "v7 id check".to_string(),
                source: UserInputSource::UserInput,
            })
            .await
            .expect("report");
        assert_uuid_v7(&mem_id);

        let start = store.start_task("v7 task").await.expect("start");
        assert_uuid_v7(&start.task_id);
    }

    #[tokio::test]
    async fn full_task_lifecycle_with_retrieval_and_weight_update() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let mem_id = store
            .report_user_input(ReportUserInput {
                lesson: "Always use Result for fallible ops".to_string(),
                source: UserInputSource::UserCorrection,
            })
            .await
            .expect("report user input");

        let start = store
            .start_task("implement error handling in parser")
            .await
            .expect("start task");
        assert!(!start.task_id.is_empty());
        assert!(
            start.memories.iter().any(|m| m.id == mem_id),
            "relevant memory should be retrieved"
        );

        let mem = start.memories.iter().find(|m| m.id == mem_id).expect("memory");
        let weight_before = mem.weight;

        store
            .end_task(
                &start.task_id,
                TaskEndInput {
                    tokens_used: 500,
                    tool_calls: 3,
                    errors: 0,
                    user_corrections: 0,
                    completed: true,
                    self_report: Some(vec![SelfReportEntry {
                        memory_id: mem_id.clone(),
                        score: 3,
                    }]),
                },
            )
            .await
            .expect("end task");

        let top = store.get_top_by_weight(5).await.expect("top");
        let updated = top.iter().find(|m| m.id == mem_id).expect("updated memory");
        let expected = update_weight(weight_before, compute_credit(1.0, 3.0, 1), 0.1);
        assert!(
            (updated.weight - expected).abs() < 1e-9,
            "weight should be updated via EMA: got {}, expected {}",
            updated.weight,
            expected
        );

        let stats = store.get_stats().await.expect("stats");
        assert_eq!(stats.task_count, 1);
        assert_eq!(stats.total_memories, 1);
        assert!(stats.avg_task_score > 0.0);
    }

    #[tokio::test]
    async fn report_correction_inserts_without_prior_task() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let id = store
            .report_correction(ReportCorrectionInput {
                lesson: "Use bcrypt".to_string(),
                what_failed: "md5".to_string(),
                what_worked: "bcrypt".to_string(),
                tokens_wasted: Some(1000),
                tools_wasted: None,
            })
            .await
            .expect("correction");

        let stats = store.get_stats().await.expect("stats");
        assert_eq!(stats.total_memories, 1, "correction insert should persist (id={id})");

        let user_id = store
            .report_user_input(ReportUserInput {
                lesson: "user lesson".to_string(),
                source: UserInputSource::UserInput,
            })
            .await
            .expect("user input");
        let stats2 = store.get_stats().await.expect("stats2");
        assert_eq!(
            stats2.total_memories, 2,
            "user insert should work alongside correction (user_id={user_id})"
        );

        let top = store.get_top_by_weight(2).await.expect("top");
        assert!(top.iter().any(|m| m.id == id));
    }

    #[tokio::test]
    async fn report_correction_sets_weight_from_tokens_wasted() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let task = store.start_task("fix auth bug").await.expect("start");
        store
            .end_task(
                &task.task_id,
                TaskEndInput {
                    tokens_used: 10_000,
                    tool_calls: 5,
                    errors: 0,
                    user_corrections: 0,
                    completed: true,
                    self_report: None,
                },
            )
            .await
            .expect("end");

        let id = store
            .report_correction(ReportCorrectionInput {
                lesson: "Use bcrypt not md5".to_string(),
                what_failed: "md5 hash".to_string(),
                what_worked: "bcrypt".to_string(),
                tokens_wasted: Some(5000),
                tools_wasted: Some(2),
            })
            .await
            .expect("correction");

        let stats = store.get_stats().await.expect("stats");
        assert_eq!(stats.total_memories, 1, "correction memory should be stored");

        let top = store.get_top_by_weight(1).await.expect("top");
        assert_eq!(top[0].id, id);
        assert!((top[0].weight - 1.5).abs() < f64::EPSILON);
        assert!(top[0].content.contains("Failed approach"));
    }

    #[tokio::test]
    async fn insert_raw_memory_and_embed_pending() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let id = store
            .insert_raw_memory("raw discovery note", MemoryCategory::Discovery, 1.5)
            .await
            .expect("insert raw");

        let n = store.embed_pending().await.expect("embed pending");
        assert_eq!(n, 1);

        let start = store.start_task("discovery task").await.expect("start");
        assert!(start.memories.iter().any(|m| m.id == id));
    }

    #[tokio::test]
    async fn purge_cleans_orphan_memory_retrievals() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let mem_id = store
            .report_user_input(ReportUserInput {
                lesson: "orphan retrieval test".to_string(),
                source: UserInputSource::UserInput,
            })
            .await
            .expect("report");

        let start = store.start_task("task with retrieval").await.expect("start");
        assert!(start.memories.iter().any(|m| m.id == mem_id));

        store
            .insert_raw_memory("purge me", MemoryCategory::Insight, 0.05)
            .await
            .expect("insert");

        let purged = store.purge(0.1).await.expect("purge");
        assert_eq!(purged, 1);

        let orphans: i64 = store
            .with_db(|conn| async move {
                let mut rows = conn
                    .query(
                        "SELECT COUNT(*) FROM memory_retrievals WHERE memory_id NOT IN (SELECT id FROM memories)",
                        (),
                    )
                    .await
                    .map_err(anyhow::Error::from)?;
                let row = rows
                    .next()
                    .await
                    .map_err(anyhow::Error::from)?
                    .ok_or_else(|| anyhow::anyhow!("no row"))?;
                row.get(0).map_err(anyhow::Error::from)
            })
            .await
            .expect("orphan count");
        assert_eq!(orphans, 0, "purge should remove orphan retrieval rows");
    }

    #[tokio::test]
    async fn decay_and_purge_maintenance() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        store
            .insert_raw_memory("low weight memory", MemoryCategory::Insight, 0.1)
            .await
            .expect("insert");

        let decayed = store.decay().await.expect("decay");
        assert_eq!(decayed.decayed, 1);

        let purged = store.purge(0.2).await.expect("purge");
        assert_eq!(purged, 1);

        let stats = store.get_stats().await.expect("stats");
        assert_eq!(stats.total_memories, 0);
    }

    #[tokio::test]
    async fn contradict_memory_deletes_and_optionally_replaces() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let id = store
            .report_user_input(ReportUserInput {
                lesson: "old fact".to_string(),
                source: UserInputSource::UserInput,
            })
            .await
            .expect("report");

        let (deleted, correction_id) = store
            .contradict_memory(&id, Some("corrected fact"))
            .await
            .expect("contradict");
        assert!(deleted);
        assert!(correction_id.is_some());

        let stats = store.get_stats().await.expect("stats");
        assert_eq!(stats.total_memories, 1);
    }

    #[tokio::test]
    async fn penalize_memory_reduces_weight_with_floor() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let id = store
            .insert_raw_memory("penalized", MemoryCategory::User, 2.0)
            .await
            .expect("insert");

        store.penalize_memory(&id, 0.25).await.expect("penalize");

        let top = store.get_top_by_weight(1).await.expect("top");
        assert!((top[0].weight - 0.5).abs() < f64::EPSILON);

        store.penalize_memory(&id, 0.01).await.expect("penalize again");
        let top = store.get_top_by_weight(1).await.expect("top");
        assert!((top[0].weight - 0.1).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn baseline_persists_across_store_instances() {
        let ctx = TestCtx::new();

        let store1 = ctx.store();
        let task = store1.start_task("first task").await.expect("start");
        store1
            .end_task(
                &task.task_id,
                TaskEndInput {
                    tokens_used: 1000,
                    tool_calls: 2,
                    errors: 1,
                    user_corrections: 0,
                    completed: true,
                    self_report: None,
                },
            )
            .await
            .expect("end");
        store1.close().await.expect("close");

        let store2 = ctx.store();
        store2.init().await.expect("re-init");
        let task2 = store2.start_task("second task").await.expect("start");
        store2
            .end_task(
                &task2.task_id,
                TaskEndInput {
                    tokens_used: 800,
                    tool_calls: 1,
                    errors: 0,
                    user_corrections: 0,
                    completed: true,
                    self_report: None,
                },
            )
            .await
            .expect("end");

        let stats = store2.get_stats().await.expect("stats");
        assert_eq!(stats.task_count, 2);
    }

    #[tokio::test]
    async fn start_task_with_no_memories_returns_empty() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let start = store.start_task("fresh task with no memories").await.expect("start");
        assert!(start.memories.is_empty());
    }

    #[tokio::test]
    async fn top_k_limits_retrieved_memories() {
        let ctx = TestCtx::new();
        let mut config = test_config(&ctx.db_path);
        config.top_k = Some(2);
        let store = ctx.store_with(config);

        for i in 0..5 {
            store
                .insert_raw_memory(&format!("memory number {i}"), MemoryCategory::Insight, 1.0)
                .await
                .expect("insert");
        }
        store.embed_pending().await.expect("embed");

        let start = store.start_task("memory number").await.expect("start");
        assert_eq!(start.memories.len(), 2);
    }

    #[tokio::test]
    async fn end_task_clears_current_task_id() {
        let ctx = TestCtx::new();
        let store = ctx.store();

        let task = store.start_task("task").await.expect("start");
        store
            .end_task(
                &task.task_id,
                TaskEndInput {
                    tokens_used: 100,
                    tool_calls: 0,
                    errors: 0,
                    user_corrections: 0,
                    completed: true,
                    self_report: None,
                },
            )
            .await
            .expect("end");

        let id = store
            .report_user_input(ReportUserInput {
                lesson: "after end".to_string(),
                source: UserInputSource::UserInput,
            })
            .await
            .expect("report");
        assert!(!id.is_empty());
    }

    #[test]
    fn is_lock_err_detects_lock_messages() {
        assert!(is_lock_err("database is locked"));
        assert!(is_lock_err("Locking error"));
        assert!(!is_lock_err("syntax error"));
    }
}
