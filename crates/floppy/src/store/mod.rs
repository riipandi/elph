mod embed;
mod read;
mod tasks;
mod write;

use anyhow::{Context, Result};
use rand::RngExt;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use std::time::{SystemTime, UNIX_EPOCH};
use turso::params;
use turso::{Builder, Connection, Database};

use super::migrations;
use super::scoring::empty_baseline;
use super::types::{FloppyConfig, TaskBaseline, VectorType};
use super::util::DEFAULT_EMBEDDING_DIMS;
use super::util::{drain_rows, retrieval_sql};

pub type EmbedFuture = Pin<Box<dyn Future<Output = Result<Vec<f32>>> + Send>>;
pub type EmbedFn = Arc<dyn Fn(&str) -> EmbedFuture + Send + Sync>;

/// Embedder that returns zero vectors (read-only inspection without a model).
pub fn noop_embedder(dimensions: u32) -> EmbedFn {
    Arc::new(move |_| {
        let dims = dimensions as usize;
        Box::pin(async move { Ok(vec![0.0f32; dims]) })
    })
}

pub(super) type WeightUpdate = (String, f64);
pub(super) type SelfReportRow = (String, u8, f64);

/// Max memories backfilled per [`MemoryStore::embed_pending`] round-trip.
pub(super) const EMBED_PENDING_BATCH: i64 = 64;

pub(super) fn in_placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(", ")
}

pub(super) async fn touch_retrieved_memories(conn: &Connection, memory_ids: &[String], now: i64) -> Result<()> {
    if memory_ids.is_empty() {
        return Ok(());
    }
    let placeholders = in_placeholders(memory_ids.len());
    let sql = format!(
        "UPDATE memories SET last_retrieved = ?, retrieval_count = retrieval_count + 1 WHERE id IN ({placeholders})"
    );
    let now_str = now.to_string();
    let mut param_refs: Vec<&str> = Vec::with_capacity(1 + memory_ids.len());
    param_refs.push(now_str.as_str());
    param_refs.extend(memory_ids.iter().map(String::as_str));
    conn.execute(&sql, turso::params_from_iter(param_refs)).await?;
    Ok(())
}

pub(super) async fn batch_set_weights(conn: &Connection, updates: &[WeightUpdate]) -> Result<()> {
    for (id, weight) in updates {
        conn.execute("UPDATE memories SET weight = ? WHERE id = ?", params![weight, id.as_str()])
            .await?;
    }
    Ok(())
}

pub(crate) fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub(super) fn new_id() -> String {
    unique_kalid()
}

fn unique_kalid() -> String {
    use std::cell::RefCell;
    use std::thread;
    use std::time::Duration;

    thread_local! {
        static LAST_KALID: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    for _ in 0..100 {
        let id = kalid::generate_kalid();
        let duplicate = LAST_KALID.with(|cell| {
            let mut last = cell.borrow_mut();
            if last.as_deref() == Some(id.as_str()) {
                true
            } else {
                *last = Some(id.clone());
                false
            }
        });
        if !duplicate {
            return id;
        }
        thread::sleep(Duration::from_millis(1));
    }
    kalid::generate_kalid()
}

/// Remove retrieval rows whose memory was deleted (prevents unbounded table growth).
pub(super) async fn delete_orphan_retrievals(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM memory_retrievals WHERE memory_id NOT IN (SELECT id FROM memories)",
        (),
    )
    .await?;
    Ok(())
}

pub(super) async fn fetch_weights(conn: &Connection, ids: &[String]) -> Result<HashMap<String, f64>> {
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
    drain_rows(&mut rows).await?;
    Ok(out)
}

pub struct MemoryStore {
    db_path: String,
    #[allow(dead_code)]
    session_id: String,
    embed: EmbedFn,
    vector_type: VectorType,
    retrieval_sql: OnceLock<Arc<str>>,
    top_k: u32,
    learning_rate: f64,
    decay_rate: f64,
    dimensions: u32,
    apply_migrations: bool,

    initialized: Mutex<bool>,
    current_task_id: Mutex<Option<String>>,
    baseline: Mutex<TaskBaseline>,
}

impl MemoryStore {
    pub fn new(config: FloppyConfig, embed: EmbedFn) -> Self {
        Self {
            db_path: config.db_path,
            session_id: config.session_id,
            embed,
            vector_type: config.vector_type.unwrap_or(VectorType::Vector32),
            retrieval_sql: OnceLock::new(),
            top_k: config.top_k.unwrap_or(5),
            learning_rate: config.learning_rate.unwrap_or(0.1),
            decay_rate: config.decay_rate.unwrap_or(0.995),
            dimensions: config.dimensions.unwrap_or(DEFAULT_EMBEDDING_DIMS),
            apply_migrations: config.apply_migrations.unwrap_or(true),
            initialized: Mutex::new(false),
            current_task_id: Mutex::new(None),
            baseline: Mutex::new(empty_baseline()),
        }
    }

    pub fn dimensions(&self) -> u32 {
        self.dimensions
    }

    pub(crate) fn vector_fn(&self) -> &'static str {
        match self.vector_type {
            VectorType::Vector32 => "vector32",
            VectorType::Vector64 => "vector64",
            VectorType::Vector8 => "vector8",
            VectorType::Vector1 => "vector1",
        }
    }

    pub(crate) fn retrieval_sql(&self) -> Arc<str> {
        self.retrieval_sql
            .get_or_init(|| Arc::from(retrieval_sql(self.vector_fn())))
            .clone()
    }

    pub(crate) fn embed_fn(&self) -> &EmbedFn {
        &self.embed
    }

    pub(crate) fn top_k(&self) -> u32 {
        self.top_k
    }

    pub(crate) fn decay_rate(&self) -> f64 {
        self.decay_rate
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
    pub(crate) async fn with_db<T, F, Fut>(&self, f: F) -> Result<T>
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
        let apply_migrations = self.apply_migrations;
        self.with_db(move |conn| async move {
            if apply_migrations {
                migrations::apply(&conn).await?;
            }

            // Load baseline
            let mut rows = conn.query("SELECT value FROM meta WHERE key = 'baseline'", ()).await?;
            let baseline = if let Some(row) = rows.next().await? {
                Some(row.get::<String>(0)?)
            } else {
                None
            };
            drain_rows(&mut rows).await?;
            Ok(baseline)
        })
        .await
        .map(|maybe_raw: Option<String>| {
            if let Some(raw) = maybe_raw
                && let Ok(b) = serde_json::from_str::<TaskBaseline>(&raw)
            {
                *self.baseline.lock().unwrap() = b;
            }
        })?;

        *self.initialized.lock().unwrap() = true;
        Ok(())
    }
}

fn is_lock_err(msg: &str) -> bool {
    msg.contains("locked") || msg.contains("Locking")
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
