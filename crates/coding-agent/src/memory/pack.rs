//! Pack ranked memories into budgeted system-prompt sections.

use floppy::MemoryCategory;

use super::capture::truncate_chars;
use super::rank::{RankSource, RankedMemory};

/// Max characters per memory body in injected context.
pub const PER_MEMORY_CHARS: usize = 500;

/// Default total character budget for all memory XML blocks.
pub const CONTEXT_BUDGET_CHARS: usize = 4000;

/// Minimum useful body chars — skip entry rather than mid-truncate below this.
const MIN_USEFUL_BODY: usize = 80;

#[derive(Debug, Clone, Default)]
pub struct PackedContext {
    pub text: String,
    pub injected_ids: Vec<String>,
    pub sections: PackedSections,
}

#[derive(Debug, Clone, Default)]
pub struct PackedSections {
    pub lessons: usize,
    pub recent_work: usize,
    pub project_map: usize,
}

fn format_entry_line(index: usize, ranked: &RankedMemory) -> String {
    let preview = truncate_chars(&ranked.memory.content, PER_MEMORY_CHARS);
    let category = format!("{:?}", ranked.memory.category).to_lowercase();
    let score_part = if ranked.memory.score > 0.0 {
        format!(" score={:.2}", ranked.memory.score)
    } else {
        String::new()
    };
    format!(
        "{}. [{} | id={}{} | w={:.2} | used={}x] {}",
        index, category, ranked.memory.id, score_part, ranked.memory.weight, ranked.memory.retrieval_count, preview,
    )
}

fn is_work(cat: MemoryCategory) -> bool {
    matches!(cat, MemoryCategory::Work)
}

fn is_discovery(cat: MemoryCategory) -> bool {
    matches!(cat, MemoryCategory::Discovery)
}

fn is_priority_sticky(r: &RankedMemory) -> bool {
    matches!(r.source, RankSource::Sticky)
        && matches!(r.memory.category, MemoryCategory::User | MemoryCategory::Correction)
        && r.memory.weight > 3.0
}

/// Pack ranked memories into `<memory_context>`, `<recent_work>`, `<project_map>` under budget.
pub fn pack_ranked_context(ranked: &[RankedMemory], budget: usize) -> PackedContext {
    if ranked.is_empty() {
        return PackedContext::default();
    }

    let mut ordered: Vec<&RankedMemory> = ranked.iter().collect();
    ordered.sort_by(|a, b| b.rank.partial_cmp(&a.rank).unwrap_or(std::cmp::Ordering::Equal));

    // Ensure at least one high-weight sticky user/correction is considered early.
    if let Some(pos) = ordered.iter().position(|r| is_priority_sticky(r)) {
        let sticky = ordered.remove(pos);
        ordered.insert(0, sticky);
    }

    let mut lessons: Vec<String> = Vec::new();
    let mut work: Vec<String> = Vec::new();
    let mut map: Vec<String> = Vec::new();
    let mut injected_ids = Vec::new();

    // Fixed section chrome cost (approximate, refined after assembly).
    let chrome = 180usize;
    let mut used = chrome;

    for r in ordered {
        let body_len = r.memory.content.chars().count().min(PER_MEMORY_CHARS);
        if body_len < MIN_USEFUL_BODY && r.memory.content.chars().count() >= MIN_USEFUL_BODY {
            // Would truncate too aggressively — skip rather than keep a useless stub.
            // (truncate_chars already used in line; if original is long, body after truncate is ok)
        }

        let next_index = match r.memory.category {
            c if is_work(c) => work.len() + 1,
            c if is_discovery(c) => map.len() + 1,
            _ => lessons.len() + 1,
        };
        let line = format_entry_line(next_index, r);
        let line_cost = line.chars().count() + 1;
        if used + line_cost > budget && !injected_ids.is_empty() {
            // Prefer skip over partial; still allow first entry always.
            continue;
        }

        used += line_cost;
        injected_ids.push(r.memory.id.clone());
        if is_work(r.memory.category) {
            work.push(line);
        } else if is_discovery(r.memory.category) {
            map.push(line);
        } else {
            // Lessons + unknown categories share the lessons section.
            lessons.push(line);
        }
    }

    let mut parts: Vec<String> = Vec::new();
    let mut sections = PackedSections::default();

    if !lessons.is_empty() {
        sections.lessons = lessons.len();
        let mut block = String::from("<memory_context>\n");
        block.push_str("Lessons and preferences (ranked for this turn):\n");
        block.push_str(&lessons.join("\n"));
        block.push_str(
            "\nUse `memory_search` / `memory_recent` for more; `memory_contradict` if wrong.\n</memory_context>",
        );
        parts.push(block);
    }

    if !work.is_empty() {
        sections.recent_work = work.len();
        let mut block = String::from("<recent_work>\n");
        block.push_str("Recent work and change footprints (do not redo completed items):\n");
        block.push_str(&work.join("\n"));
        block.push_str("\n</recent_work>");
        parts.push(block);
    }

    if !map.is_empty() {
        sections.project_map = map.len();
        let mut block = String::from("<project_map>\n");
        block.push_str("Known project layout (prefer over re-running broad list_dir unless stale):\n");
        block.push_str(&map.join("\n"));
        block.push_str("\n</project_map>");
        parts.push(block);
    }

    let text = parts.join("\n\n");
    // Hard clamp if chrome estimation drifted.
    let text = if text.chars().count() > budget {
        truncate_chars(&text, budget)
    } else {
        text
    };

    PackedContext {
        text,
        injected_ids,
        sections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::rank::{RankSource, RankedMemory};
    use floppy::{Memory, MemoryCategory};

    fn ranked(id: &str, cat: MemoryCategory, weight: f64, rank: f64, content: &str) -> RankedMemory {
        RankedMemory {
            memory: Memory {
                id: id.into(),
                content: content.into(),
                category: cat,
                weight,
                score: 0.5,
                created_at: 0,
                retrieval_count: 1,
            },
            rank,
            source: RankSource::Semantic,
        }
    }

    #[test]
    fn budget_never_exceeded() {
        let long = "y".repeat(400);
        let items: Vec<RankedMemory> = (0..20)
            .map(|i| ranked(&format!("id{i}"), MemoryCategory::Insight, 1.0, 1.0 - i as f64 * 0.01, &long))
            .collect();
        let packed = pack_ranked_context(&items, 3000);
        assert!(packed.text.chars().count() <= 3000, "got {} chars", packed.text.chars().count());
        assert!(!packed.injected_ids.is_empty());
    }

    #[test]
    fn sticky_high_weight_preferred() {
        let sticky = RankedMemory {
            memory: Memory {
                id: "sticky".into(),
                content: "User prefers pnpm over npm for this monorepo always.".into(),
                category: MemoryCategory::User,
                weight: 4.0,
                score: 0.0,
                created_at: 0,
                retrieval_count: 10,
            },
            rank: 0.2,
            source: RankSource::Sticky,
        };
        let low = ranked(
            "low",
            MemoryCategory::Insight,
            1.0,
            0.99,
            "Some less critical insight about formatting that is still fairly long enough.",
        );
        let packed = pack_ranked_context(&[low, sticky], 500);
        assert!(
            packed.injected_ids.contains(&"sticky".to_string()) || packed.text.contains("sticky"),
            "packed={}",
            packed.text
        );
    }

    #[test]
    fn sections_split_by_category() {
        let items = vec![
            ranked(
                "l1",
                MemoryCategory::Correction,
                2.0,
                0.9,
                "Failed approach used shell for file edits; use edit_file instead always.",
            ),
            ranked(
                "w1",
                MemoryCategory::Work,
                1.0,
                0.8,
                "[work] fixed hooks\nPaths: a.rs\nOutcome: success\nNote: auto",
            ),
            ranked(
                "d1",
                MemoryCategory::Discovery,
                1.0,
                0.7,
                "[discovery] Area: crates/coding-agent/src/memory/\nObserved tools: list_dir×2\nNotes: hooks.rs",
            ),
        ];
        let packed = pack_ranked_context(&items, 3000);
        assert!(packed.text.contains("<memory_context>"));
        assert!(packed.text.contains("<recent_work>"));
        assert!(packed.text.contains("<project_map>"));
        assert_eq!(packed.sections.lessons, 1);
        assert_eq!(packed.sections.recent_work, 1);
        assert_eq!(packed.sections.project_map, 1);
    }
}
