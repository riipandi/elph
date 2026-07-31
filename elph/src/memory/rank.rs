//! Multi-source memory ranking: semantic + recent + sticky.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use floppy::{Memory, MemoryCategory, MemoryRecord};

/// Half-life for recency boost (seconds) — ~7 days.
pub const RECENCY_HALF_LIFE_SECS: f64 = 7.0 * 86_400.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankSource {
    Semantic,
    Recent,
    Sticky,
}

#[derive(Debug, Clone)]
pub struct RankedMemory {
    pub memory: Memory,
    pub rank: f64,
    pub source: RankSource,
}

#[derive(Debug, Clone)]
pub struct RankOptions {
    pub prompt: String,
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
}

impl Default for RankOptions {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            alpha: 0.50,
            beta: 0.25,
            gamma: 0.25,
        }
    }
}

impl RankOptions {
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }
}

/// Detect continuation-style prompts (prefer recent work).
pub fn is_continuation_prompt(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    const CUES: &[&str] = &[
        "continue",
        "next",
        "fix remaining",
        "lanjut",
        "keep going",
        "go on",
        "resume",
        "carry on",
        "what's left",
        "what is left",
        "sisa",
        "lanjutkan",
    ];
    CUES.iter().any(|c| lower.contains(c))
}

/// Adjust adaptive recall threshold for continuation prompts (lower = more permissive).
pub fn adaptive_threshold_adjustment(prompt: &str) -> f64 {
    if is_continuation_prompt(prompt) { -0.05 } else { 0.0 }
}

/// Detect structure / layout questions (prefer discovery / project map).
pub fn is_structure_prompt(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    const CUES: &[&str] = &[
        "where is",
        "where are",
        "structure",
        "layout",
        "arsitektur",
        "architecture",
        "module map",
        "project map",
        "directory tree",
        "di mana",
        "dimana",
        "letaknya",
        "folder structure",
    ];
    CUES.iter().any(|c| lower.contains(c))
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Normalize weight from floppy clamp range [0.1, 5.0] into [0, 1].
pub fn normalize_weight(weight: f64) -> f64 {
    ((weight - 0.1) / (5.0 - 0.1)).clamp(0.0, 1.0)
}

/// Exponential recency boost in [0, 1]; half-life ~7d.
pub fn recency_boost(created_at: i64, now: i64) -> f64 {
    let age = (now - created_at).max(0) as f64;
    (-age * std::f64::consts::LN_2 / RECENCY_HALF_LIFE_SECS).exp()
}

fn category_boost(category: MemoryCategory, continuation: bool, structure: bool) -> f64 {
    match category {
        MemoryCategory::User | MemoryCategory::Correction => 1.15,
        MemoryCategory::Work if continuation => 1.20,
        MemoryCategory::Discovery if structure => 1.10,
        MemoryCategory::Work => 1.05,
        MemoryCategory::Discovery => 1.0,
        _ => 1.0,
    }
}

fn record_to_memory(r: &MemoryRecord, score: f64) -> Memory {
    Memory {
        id: r.id.clone(),
        content: r.content.clone(),
        category: r.category,
        weight: r.weight,
        score,
        created_at: r.created_at,
        retrieval_count: r.retrieval_count,
    }
}

fn score_one(
    mem: &Memory,
    source: RankSource,
    now: i64,
    opts: &RankOptions,
    continuation: bool,
    structure: bool,
) -> f64 {
    let similarity = match source {
        RankSource::Semantic => mem.score.clamp(0.0, 1.0),
        RankSource::Recent => 0.0,
        RankSource::Sticky => 0.15, // small base so sticky still ranks without semantic hit
    };
    let base = opts.alpha * similarity
        + opts.beta * normalize_weight(mem.weight)
        + opts.gamma * recency_boost(mem.created_at, now);
    base * category_boost(mem.category, continuation, structure)
}

/// Merge semantic, recent, and sticky sources; dedupe by id (keep highest rank).
pub fn merge_and_rank(
    semantic: Vec<Memory>,
    recent: Vec<MemoryRecord>,
    sticky: Vec<Memory>,
    now: i64,
    opts: &RankOptions,
) -> Vec<RankedMemory> {
    let continuation = is_continuation_prompt(&opts.prompt);
    let structure = is_structure_prompt(&opts.prompt);

    let mut best: HashMap<String, RankedMemory> = HashMap::new();

    let consider = |best: &mut HashMap<String, RankedMemory>, mem: Memory, source: RankSource| {
        let rank = score_one(&mem, source, now, opts, continuation, structure);
        let entry = RankedMemory {
            memory: mem,
            rank,
            source,
        };
        best.entry(entry.memory.id.clone())
            .and_modify(|existing| {
                if entry.rank > existing.rank {
                    *existing = RankedMemory {
                        memory: entry.memory.clone(),
                        rank: entry.rank,
                        source: entry.source,
                    };
                }
            })
            .or_insert(entry);
    };

    for m in semantic {
        consider(&mut best, m, RankSource::Semantic);
    }
    for r in &recent {
        // Recent work/discovery often has no similarity; use mild synthetic score for ranking display.
        consider(&mut best, record_to_memory(r, 0.0), RankSource::Recent);
    }
    for m in sticky {
        consider(&mut best, m, RankSource::Sticky);
    }

    let mut out: Vec<RankedMemory> = best.into_values().collect();
    out.sort_by(|a, b| b.rank.partial_cmp(&a.rank).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Filter sticky candidates to correction / user / insight only.
pub fn filter_sticky(memories: Vec<Memory>) -> Vec<Memory> {
    memories
        .into_iter()
        .filter(|m| {
            matches!(
                m.category,
                MemoryCategory::Correction | MemoryCategory::User | MemoryCategory::Insight
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(id: &str, cat: MemoryCategory, weight: f64, score: f64, created: i64) -> Memory {
        Memory {
            id: id.into(),
            content: format!("content-{id}"),
            category: cat,
            weight,
            score,
            created_at: created,
            retrieval_count: 1,
        }
    }

    fn rec(id: &str, cat: MemoryCategory, weight: f64, created: i64) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            content: format!("content-{id}"),
            category: cat,
            weight,
            retrieval_count: 1,
            created_at: created,
            embedding_status: floppy::EmbeddingStatus::Ok,
        }
    }

    #[test]
    fn merge_dedupes_by_id_keeps_higher_rank() {
        let now = 1_000_000;
        let semantic = vec![mem("a", MemoryCategory::Insight, 1.0, 0.9, now)];
        let sticky = vec![mem("a", MemoryCategory::Insight, 1.0, 0.1, now)];
        let ranked = merge_and_rank(semantic, vec![], sticky, now, &RankOptions::default());
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].memory.id, "a");
        assert_eq!(ranked[0].source, RankSource::Semantic);
    }

    #[test]
    fn continuation_boost_prefers_work() {
        let now = 1_000_000;
        let recent = vec![rec("w1", MemoryCategory::Work, 1.0, now)];
        let semantic = vec![mem("i1", MemoryCategory::Insight, 1.0, 0.4, now - 30 * 86_400)];
        let opts = RankOptions::default().with_prompt("please continue the remaining work");
        let ranked = merge_and_rank(semantic, recent, vec![], now, &opts);
        assert!(ranked.iter().any(|r| r.memory.id == "w1"));
        // Work entry should rank at least as high as low-similarity old insight
        let work_rank = ranked.iter().find(|r| r.memory.id == "w1").unwrap().rank;
        let insight_rank = ranked.iter().find(|r| r.memory.id == "i1").unwrap().rank;
        assert!(work_rank >= insight_rank * 0.9);
    }

    #[test]
    fn structure_cues_detected() {
        assert!(is_structure_prompt("where is the memory module?"));
        assert!(is_structure_prompt("jelaskan arsitektur folder"));
        assert!(!is_structure_prompt("fix the clippy error"));
    }
}
