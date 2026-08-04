mod embed;
mod read;
mod tasks;
mod write;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use std::time::{SystemTime, UNIX_EPOCH};
use turso::params;
use turso::{Connection, Database};
use turso_db::clear_broken_wal_sidecars;

pub use crate::core::embed::EmbedFn;
use crate::core::util::{DEFAULT_EMBEDDING_DIMS, drain_rows};
use crate::memory::migrations;
use crate::memory::scoring::empty_baseline;
use crate::memory::types::{FloppyConfig, Memory, TaskBaseline, VectorType};
use crate::memory::util::{category_from_str, retrieval_sql};

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

/// Cosine similarity (0–1) for FTS-only ids that have embeddings.
///
/// Per-id indexed lookups; the candidate window is small (`top_k * 4`, capped).
async fn keyword_cosines(conn: &Connection, ids: &[String], emb_buf: &[u8], vfn: &str) -> Result<HashMap<String, f64>> {
    let mut out = HashMap::with_capacity(ids.len());
    for id in ids {
        let mut rows = conn
            .query(
                &format!(
                    "SELECT vector_distance_cos({vfn}(embedding), {vfn}(?)) FROM memories \
                     WHERE id = ? AND embedding IS NOT NULL"
                ),
                params![emb_buf, id.as_str()],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            let distance: f64 = row.get(0)?;
            out.insert(id.clone(), 1.0 - distance);
        }
        drain_rows(&mut rows).await?;
    }
    Ok(out)
}

/// Rank-based keyword confidence for FTS-only hits without embeddings.
///
/// Decays from 0.55 (top hit) to a 0.35 floor: strong enough to pass the low
/// end of the adaptive-recall threshold, never high enough to outrank a
/// genuine semantic match.
fn keyword_score(rank: usize) -> f64 {
    (0.55 * (1.0 - 0.15 * rank as f64)).max(0.35)
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

    /// Active task id used to link new memories via `source_task`.
    pub fn current_task_id(&self) -> Option<String> {
        self.current_task_id.lock().unwrap().clone()
    }

    /// Align host-managed task lifecycle with the store (shared runtime).
    pub fn set_current_task_id(&self, task_id: Option<String>) {
        *self.current_task_id.lock().unwrap() = task_id;
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

    /// Hybrid keyword + vector retrieval, preserving `Memory.score` as cosine
    /// similarity (0–1).
    ///
    /// The vector path (cosine, recency-decayed) always runs. When the
    /// Turso-native FTS index is available, a keyword pass on `content` adds
    /// exact matches the vector search missed: hits that have embeddings keep
    /// their real cosine score; hits without embeddings (pending/truncated)
    /// get a rank-based keyword score in [0.35, 0.55] so they stay below strong
    /// semantic matches but above the adaptive-recall floor.
    pub(crate) async fn hybrid_retrieve(
        &self,
        conn: &Connection,
        query: &str,
        emb_buf: &[u8],
        decay_rate: f64,
        now: i64,
    ) -> Result<Vec<Memory>> {
        let top_k = self.top_k();
        let vfn = self.vector_fn();
        let sql = self.retrieval_sql();
        let mut by_id: HashMap<String, Memory> = HashMap::new();

        // Vector path (existing semantics).
        {
            let mut rows = conn
                .query(sql.as_ref(), params![emb_buf, emb_buf, decay_rate, now, top_k])
                .await?;
            while let Some(row) = rows.next().await? {
                let distance: f64 = row.get(6)?;
                by_id.insert(
                    row.get::<String>(0)?,
                    Memory {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        category: category_from_str(&row.get::<String>(2)?),
                        weight: row.get(3)?,
                        score: 1.0 - distance,
                        created_at: row.get(4)?,
                        retrieval_count: row.get(5)?,
                    },
                );
            }
            drain_rows(&mut rows).await?;
        }

        // Keyword path (Tantivy). Only surfaces hits vector search missed; the
        // FTS index is auto-maintained on insert/update/delete.
        if migrations::fts_available(conn).await.unwrap_or(false) {
            let fts_q = crate::core::fts::sanitize_query(query);
            if !fts_q.is_empty() {
                let cand = (top_k * 4).max(20) as usize;
                let sql = "SELECT * FROM memories WHERE fts_match(content, ?1)";
                if let Ok(mut rows) = conn.query(sql, params![fts_q.as_str()]).await {
                    // FTS hits in BM25 order; `SELECT *` is required by the
                    // Turso FTS rewrite, so read by `memories` column index.
                    let mut hits: Vec<(String, String, String, f64, i64, u32)> = Vec::new();
                    let mut i = 0usize;
                    while let Some(row) = rows.next().await? {
                        i += 1;
                        if i > cand {
                            break;
                        }
                        hits.push((
                            row.get(0)?, // id
                            row.get(1)?, // content
                            row.get(3)?, // category
                            row.get(4)?, // weight
                            row.get(6)?, // created_at
                            row.get(8)?, // retrieval_count
                        ));
                    }
                    drain_rows(&mut rows).await?;

                    // Cosine for FTS-only ids that do have embeddings.
                    let missing: Vec<String> = hits
                        .iter()
                        .filter(|(id, ..)| !by_id.contains_key(id))
                        .map(|(id, ..)| id.clone())
                        .collect();
                    let cosines = keyword_cosines(conn, &missing, emb_buf, vfn).await?;

                    for (rank, (id, content, category, weight, created_at, retrieval_count)) in
                        hits.into_iter().enumerate()
                    {
                        if by_id.contains_key(&id) {
                            continue;
                        }
                        let score = cosines.get(&id).copied().unwrap_or_else(|| keyword_score(rank));
                        by_id.insert(
                            id.clone(),
                            Memory {
                                id,
                                content,
                                category: category_from_str(&category),
                                weight,
                                score,
                                created_at,
                                retrieval_count,
                            },
                        );
                    }
                }
            }
        }

        let mut out: Vec<Memory> = by_id.into_values().collect();
        out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(top_k as usize);
        Ok(out)
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
        // Parent dir must exist; Turso creates `store.db` on first open but not parents.
        if let Some(parent) = Path::new(&self.db_path).parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create memory store directory {}", parent.display()))?;
        }

        // Drop empty/truncated WAL sidecars *before* first open — a 0-byte
        // `store.db-wal` makes Turso fail with "short read on WAL frame" even
        // when `store.db` itself is healthy (or when the main file is missing).
        clear_broken_wal_sidecars(&self.db_path);

        turso_db::open_local(
            Path::new(&self.db_path),
            |b| b.experimental_multiprocess_wal(true).experimental_index_method(true),
            true,
        )
        .await
    }
    pub(crate) async fn with_db<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Connection) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let db = self.open_db().await?;
        let conn = turso_db::connect(&db).await?;
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

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
