use std::time::{SystemTime, UNIX_EPOCH};

use elph_core::floppy::category_str;
use elph_core::floppy::{
    EmbeddingStatus, MemoryCategory, MemoryRecord, StoreStatus, TaskRecord, TaskStatus, TimelineEvent,
    TimelineEventKind,
};

pub fn time_ago(epoch_secs: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(epoch_secs);
    let diff = (now - epoch_secs).max(0);
    if diff < 60 {
        format!("{diff}s ago")
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86_400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86_400)
    }
}

pub fn embedding_label(status: EmbeddingStatus) -> &'static str {
    match status {
        EmbeddingStatus::Ok => "OK",
        EmbeddingStatus::Pending => "pending",
        EmbeddingStatus::Truncated => "truncated",
    }
}

pub fn parse_category_filter(raw: &str) -> Option<MemoryCategory> {
    match raw {
        "correction" => Some(MemoryCategory::Correction),
        "insight" => Some(MemoryCategory::Insight),
        "user" => Some(MemoryCategory::User),
        "consolidated" => Some(MemoryCategory::Consolidated),
        "discovery" => Some(MemoryCategory::Discovery),
        _ => None,
    }
}

pub fn print_status(status: &StoreStatus) {
    println!("floppy status:");
    println!("  Memories:  {}", status.total_memories);
    println!("  Tasks:     {}", status.completed_tasks);
    let avg = if status.avg_task_score.is_finite() && status.total_tasks > 0 {
        format!("{:.3}", status.avg_task_score)
    } else {
        "N/A".into()
    };
    println!("  Avg score: {avg}");

    if !status.categories.is_empty() {
        let cats = status
            .categories
            .iter()
            .map(|c| format!("{}={}", category_str(c.category), c.count))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  By category: {cats}");
    }

    if !status.top_memories.is_empty() {
        println!("\n  Top by weight:");
        for m in &status.top_memories {
            let preview = truncate(&m.content, 70);
            println!("    [w={:.2}, used={}x] {preview}", m.weight, m.retrieval_count);
        }
    }
}

pub fn print_memories(records: &[MemoryRecord], filter: Option<MemoryCategory>) {
    if records.is_empty() {
        let label = filter.map(category_str).unwrap_or("all");
        println!("No {label} memories found.");
        return;
    }

    let suffix = filter.map(|c| format!(" ({})", category_str(c))).unwrap_or_default();
    println!("{} memories{suffix}:\n", records.len());

    for r in records {
        println!(
            "--- [{}] w={:.2} | used={}x | emb={} | {} ---",
            category_str(r.category),
            r.weight,
            r.retrieval_count,
            embedding_label(r.embedding_status),
            time_ago(r.created_at),
        );
        let body = if r.content.len() > 500 {
            format!("{}\n  ...({} chars total)", &r.content[..500], r.content.len())
        } else {
            r.content.clone()
        };
        println!("{body}\n");
    }
}

pub fn print_tasks(tasks: &[TaskRecord]) {
    if tasks.is_empty() {
        println!("No tasks found.");
        return;
    }

    println!("Last {} tasks:\n", tasks.len());

    for t in tasks {
        let status = match t.status {
            TaskStatus::InProgress => "in-progress",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
        };
        let score = t.task_score.map(|s| format!("{s:.3}")).unwrap_or_else(|| "N/A".into());
        let tokens = t.tokens_used.map(|n| n.to_string()).unwrap_or_else(|| "?".into());
        let calls = t.tool_calls.map(|n| n.to_string()).unwrap_or_else(|| "?".into());
        let errors = t.errors.unwrap_or(0);
        let corr = t.user_corrections.unwrap_or(0);
        let when = t.started_at.map(time_ago).unwrap_or_else(|| "?".into());
        let desc = truncate(t.description.as_deref().unwrap_or(""), 100);

        println!("[{status}] score={score} | {tokens}tok, {calls}calls, {errors}err, {corr}corr | {when}");
        println!("  {desc}");

        for r in &t.retrievals {
            let rated = r.self_report.map(|s| format!(" rated={s}/3")).unwrap_or_default();
            let credit = r.credit.map(|c| format!(" credit={c:.2}")).unwrap_or_default();
            let sim = r.similarity.unwrap_or(0.0);
            println!(
                "    -> [{}] sim={sim:.3}{rated}{credit} \"{}...\"",
                category_str(r.category),
                r.preview,
            );
        }

        for c in &t.created_memories {
            println!("    <- stored [{}] \"{}...\"", category_str(c.category), c.preview,);
        }

        println!();
    }
}

pub fn print_timeline(events: &[TimelineEvent]) {
    if events.is_empty() {
        println!("Timeline is empty.");
        return;
    }

    println!("Timeline:\n");
    for e in events {
        let when = time_ago(e.timestamp);
        let prefix = match e.kind {
            TimelineEventKind::Task => "TASK",
            TimelineEventKind::Memory => "MEM ",
        };
        println!("{when:>8}  {prefix}  {}", e.summary);
    }
}

pub fn print_search_results(query: &str, memories: &[elph_core::floppy::Memory]) {
    if memories.is_empty() {
        println!("No relevant memories found.");
        return;
    }

    println!("Top {} results for \"{query}\":\n", memories.len());
    for m in memories {
        println!("[{}] score={:.3} w={:.2}", category_str(m.category), m.score, m.weight,);
        println!("  {}\n", truncate(&m.content, 200));
    }
}

pub fn print_purge(count: u32, threshold: f64) {
    println!("Purged {count} memories below weight {threshold}");
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_category_filter_accepts_known_values() {
        assert_eq!(parse_category_filter("user"), Some(MemoryCategory::User));
        assert_eq!(parse_category_filter("nope"), None);
    }

    #[test]
    fn truncate_shortens_long_strings() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello...");
    }
}
