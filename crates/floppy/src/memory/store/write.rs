use anyhow::Result;
use turso::params;

use super::MemoryStore;
use super::delete_orphan_retrievals;
use super::{new_id, now_secs};
use crate::core::util::drain_rows;
use crate::memory::types::{ConsolidateResult, DecayResult, FlushResult};

impl MemoryStore {
    /// Category-aware weight decay.
    ///
    /// - `work`: faster fade (`min(decay_rate, 0.98)`)
    /// - `correction` / `user`: slower fade (`max(decay_rate, 0.998)`)
    /// - others: base `decay_rate`
    pub async fn decay(&self) -> Result<DecayResult> {
        self.init().await?;
        let base = self.decay_rate;
        let work_rate = base.min(0.98);
        let sticky_rate = base.max(0.998);
        let other_rate = base;
        self.with_db(move |conn| async move {
            let decayed = conn
                .execute(
                    r#"
                    UPDATE memories SET weight = weight * CASE
                        WHEN category = 'work' THEN ?
                        WHEN category IN ('correction', 'user') THEN ?
                        ELSE ?
                    END
                    "#,
                    params![work_rate, sticky_rate, other_rate],
                )
                .await?;
            // Standard weak-memory purge.
            let mut deleted = conn
                .execute("DELETE FROM memories WHERE weight < 0.15 AND retrieval_count > 5", ())
                .await?;
            // Extra: drop very old, barely-used work notes (ephemeral operational state).
            let now = now_secs();
            let work_cutoff = now - 14 * 86_400;
            deleted += conn
                .execute(
                    "DELETE FROM memories WHERE category = 'work' AND weight < 0.4 AND created_at < ? AND retrieval_count < 3",
                    params![work_cutoff],
                )
                .await?;
            delete_orphan_retrievals(&conn).await?;
            Ok(DecayResult {
                decayed: decayed as u32,
                deleted: deleted as u32,
            })
        })
        .await
        .inspect(|r| {
            if r.decayed > 0 || r.deleted > 0 {
                log::debug!("memory decay decayed={} deleted={}", r.decayed, r.deleted);
            }
        })
    }

    /// Merge near-duplicate memories (same category, high embedding similarity, low weight).
    ///
    /// Conservative MVP: at most `max_merges` pairs; only weights below `max_weight`.
    /// `distance_threshold` is cosine **distance** (1 - similarity); lower = stricter.
    pub async fn consolidate_similar(&self, distance_threshold: f64, max_merges: u32) -> Result<ConsolidateResult> {
        self.init().await?;
        const MAX_WEIGHT: f64 = 2.5;
        const BATCH: i64 = 200;
        let max_merges = max_merges.min(10);

        let rows: Vec<(String, String, String, f64, Vec<u8>)> = self
            .with_db(|conn| async move {
                let mut r = conn
                    .query(
                        "SELECT id, content, category, weight, embedding FROM memories \
                         WHERE embedding IS NOT NULL AND category != 'consolidated' \
                         ORDER BY created_at DESC LIMIT ?",
                        params![BATCH],
                    )
                    .await?;
                let mut out = Vec::new();
                while let Some(row) = r.next().await? {
                    let emb: Vec<u8> = row.get(4)?;
                    out.push((
                        row.get::<String>(0)?,
                        row.get::<String>(1)?,
                        row.get::<String>(2)?,
                        row.get::<f64>(3)?,
                        emb,
                    ));
                }
                drain_rows(&mut r).await?;
                Ok(out)
            })
            .await?;

        use std::collections::HashMap;
        let mut by_cat: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, row) in rows.iter().enumerate() {
            if row.3 >= MAX_WEIGHT {
                continue;
            }
            by_cat.entry(row.2.clone()).or_default().push(i);
        }

        let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
        for idxs in by_cat.values() {
            for a in 0..idxs.len() {
                for b in (a + 1)..idxs.len() {
                    let i = idxs[a];
                    let j = idxs[b];
                    let dist = cosine_distance_bytes(&rows[i].4, &rows[j].4);
                    if dist <= distance_threshold {
                        pairs.push((i, j, dist));
                    }
                }
            }
        }
        pairs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        let mut used = std::collections::HashSet::new();
        let mut merged = 0u32;
        let mut deleted = 0u32;

        for (i, j, _) in pairs {
            if merged >= max_merges {
                break;
            }
            if used.contains(&i) || used.contains(&j) {
                continue;
            }
            let (id_a, content_a, _, w_a, _) = &rows[i];
            let (id_b, content_b, _, w_b, emb_b) = &rows[j];
            let summary = format!(
                "[consolidated] {}\n---\n{}",
                truncate_for_merge(content_a, 220),
                truncate_for_merge(content_b, 220),
            );
            let weight = ((w_a + w_b) / 2.0).clamp(0.1, 5.0);
            let emb = emb_b.clone();
            let id_a = id_a.clone();
            let id_b = id_b.clone();
            let new_id = new_id();
            let now = now_secs();

            let ok = self
                .with_db({
                    let summary = summary.clone();
                    let emb = emb.clone();
                    let id_a = id_a.clone();
                    let id_b = id_b.clone();
                    move |conn| async move {
                        conn.execute(
                            "INSERT INTO memories (id, content, embedding, category, weight, created_at, source_task) \
                             VALUES (?, ?, ?, 'consolidated', ?, ?, NULL)",
                            params![new_id.as_str(), summary.as_str(), emb.as_slice(), weight, now],
                        )
                        .await?;
                        conn.execute("DELETE FROM memories WHERE id = ?", params![id_a.as_str()])
                            .await?;
                        conn.execute("DELETE FROM memories WHERE id = ?", params![id_b.as_str()])
                            .await?;
                        delete_orphan_retrievals(&conn).await?;
                        Ok(())
                    }
                })
                .await;

            if ok.is_ok() {
                used.insert(i);
                used.insert(j);
                merged += 1;
                deleted += 2;
            }
        }

        if merged > 0 {
            log::debug!("memory consolidate merged={merged} deleted={deleted}");
        }
        Ok(ConsolidateResult { merged, deleted })
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
        .inspect(|&n| {
            if n > 0 {
                log::info!("memory purge deleted={n}");
            }
        })
    }

    /// Delete **all** memories, retrieval links, and tasks (full store wipe).
    ///
    /// Unlike [`Self::purge`], this ignores weight and does not leave any rows.
    /// Schema / `meta` are preserved.
    pub async fn flush(&self) -> Result<FlushResult> {
        self.init().await?;
        self.with_db(move |conn| async move {
            // Count first so the result is meaningful even when tables are empty.
            let mut mem_rows = conn.query("SELECT COUNT(*) FROM memories", ()).await?;
            let memories = match mem_rows.next().await? {
                Some(row) => row.get::<i64>(0)? as u32,
                None => 0,
            };
            let mut task_rows = conn.query("SELECT COUNT(*) FROM tasks", ()).await?;
            let tasks = match task_rows.next().await? {
                Some(row) => row.get::<i64>(0)? as u32,
                None => 0,
            };

            conn.execute("DELETE FROM memory_retrievals", ()).await?;
            conn.execute("DELETE FROM memories", ()).await?;
            conn.execute("DELETE FROM tasks", ()).await?;
            Ok(FlushResult { memories, tasks })
        })
        .await
        .inspect(|r| {
            log::info!("memory flush memories={} tasks={}", r.memories, r.tasks);
        })
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
        log::debug!("memory store closed path={}", self.db_path);
        Ok(())
    }
}

fn cosine_distance_bytes(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.len() < 4 || !a.len().is_multiple_of(4) {
        return 1.0;
    }
    let n = a.len() / 4;
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for i in 0..n {
        let o = i * 4;
        let fa = f32::from_le_bytes([a[o], a[o + 1], a[o + 2], a[o + 3]]) as f64;
        let fb = f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as f64;
        dot += fa * fb;
        na += fa * fa;
        nb += fb * fb;
    }
    if na <= f64::EPSILON || nb <= f64::EPSILON {
        return 1.0;
    }
    let sim = (dot / (na.sqrt() * nb.sqrt())).clamp(-1.0, 1.0);
    1.0 - sim
}

fn truncate_for_merge(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}...")
    }
}
