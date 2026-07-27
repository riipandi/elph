use std::time::{SystemTime, UNIX_EPOCH};

use floppy::category_str;
use floppy::{
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

pub fn write_status(out: &mut String, status: &StoreStatus) {
    use std::fmt::Write;
    let _ = writeln!(out, "floppy status:");
    let _ = writeln!(out, "  Memories:  {}", status.total_memories);
    let _ = writeln!(out, "  Tasks:     {}", status.completed_tasks);
    let avg = if status.avg_task_score.is_finite() && status.total_tasks > 0 {
        format!("{:.3}", status.avg_task_score)
    } else {
        "N/A".into()
    };
    let _ = writeln!(out, "  Avg score: {avg}");

    if !status.categories.is_empty() {
        let cats = status
            .categories
            .iter()
            .map(|c| format!("{}={}", category_str(c.category), c.count))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "  By category: {cats}");
    }

    if !status.top_memories.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Top by weight:");
        for m in &status.top_memories {
            let preview = truncate(&m.content, 70);
            let _ = writeln!(out, "    [w={:.2}, used={}x] {preview}", m.weight, m.retrieval_count);
        }
    }
}

pub fn write_memories(out: &mut String, records: &[MemoryRecord], filter: Option<MemoryCategory>) {
    use std::fmt::Write;
    if records.is_empty() {
        let label = filter.map(category_str).unwrap_or("all");
        let _ = writeln!(out, "No {label} memories found.");
        return;
    }

    let suffix = filter.map(|c| format!(" ({})", category_str(c))).unwrap_or_default();
    let _ = writeln!(out, "{} memories{suffix}:", records.len());
    let _ = writeln!(out);

    for r in records {
        let _ = writeln!(
            out,
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
        let _ = writeln!(out, "{body}\n");
    }
}

pub fn write_tasks(out: &mut String, tasks: &[TaskRecord]) {
    use std::fmt::Write;
    if tasks.is_empty() {
        let _ = writeln!(out, "No tasks found.");
        return;
    }

    let _ = writeln!(out, "Last {} tasks:", tasks.len());
    let _ = writeln!(out);

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

        let _ = writeln!(
            out,
            "[{status}] score={score} | {tokens}tok, {calls}calls, {errors}err, {corr}corr | {when}"
        );
        let _ = writeln!(out, "  {desc}");

        for r in &t.retrievals {
            let rated = r.self_report.map(|s| format!(" rated={s}/3")).unwrap_or_default();
            let credit = r.credit.map(|c| format!(" credit={c:.2}")).unwrap_or_default();
            let sim = r.similarity.unwrap_or(0.0);
            let _ = writeln!(
                out,
                "    -> [{}] sim={sim:.3}{rated}{credit} \"{}...\"",
                category_str(r.category),
                r.preview,
            );
        }

        for c in &t.created_memories {
            let _ = writeln!(out, "    <- stored [{}] \"{}...\"", category_str(c.category), c.preview,);
        }

        let _ = writeln!(out);
    }
}

pub fn write_timeline(out: &mut String, events: &[TimelineEvent]) {
    use std::fmt::Write;
    if events.is_empty() {
        let _ = writeln!(out, "Timeline is empty.");
        return;
    }

    let _ = writeln!(out, "Timeline:");
    let _ = writeln!(out);
    for e in events {
        let when = time_ago(e.timestamp);
        let prefix = match e.kind {
            TimelineEventKind::Task => "TASK",
            TimelineEventKind::Memory => "MEM ",
        };
        let _ = writeln!(out, "{when:>8}  {prefix}  {}", e.summary);
    }
}

pub fn write_search_results(out: &mut String, query: &str, memories: &[floppy::Memory]) {
    use std::fmt::Write;
    if memories.is_empty() {
        let _ = writeln!(out, "No relevant memories found.");
        return;
    }

    let _ = writeln!(out, "Top {} results for \"{query}\":", memories.len());
    let _ = writeln!(out);
    for m in memories {
        let _ = writeln!(out, "[{}] score={:.3} w={:.2}", category_str(m.category), m.score, m.weight,);
        let _ = writeln!(out, "  {}\n", truncate(&m.content, 200));
    }
}

pub fn write_purge(out: &mut String, count: u32, threshold: f64) {
    use std::fmt::Write;
    let _ = writeln!(out, "Purged {count} memories below weight {threshold}");
}

pub fn print_status(status: &StoreStatus) {
    let mut buf = String::new();
    write_status(&mut buf, status);
    print!("{buf}");
}

pub fn print_memories(records: &[MemoryRecord], filter: Option<MemoryCategory>) {
    let mut buf = String::new();
    write_memories(&mut buf, records, filter);
    print!("{buf}");
}

pub fn print_tasks(tasks: &[TaskRecord]) {
    let mut buf = String::new();
    write_tasks(&mut buf, tasks);
    print!("{buf}");
}

pub fn print_timeline(events: &[TimelineEvent]) {
    let mut buf = String::new();
    write_timeline(&mut buf, events);
    print!("{buf}");
}

pub fn print_search_results(query: &str, memories: &[floppy::Memory]) {
    let mut buf = String::new();
    write_search_results(&mut buf, query, memories);
    print!("{buf}");
}

pub fn print_purge(count: u32, threshold: f64) {
    let mut buf = String::new();
    write_purge(&mut buf, count, threshold);
    print!("{buf}");
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
