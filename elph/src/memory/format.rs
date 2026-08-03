//! Human-readable formatting for memory CLI and `/memory` slash output.
//!
//! CLI output uses ANSI colors when stdout is a TTY (respects `NO_COLOR`).
//! Slash / dialog output stays plain so scroll dialogs do not show escape codes.

use std::fmt;
use std::io::IsTerminal;
use std::time::{SystemTime, UNIX_EPOCH};

use anstyle::{AnsiColor, Color, Style};
use floppy::category_str;
use floppy::{
    EmbeddingStatus, Memory, MemoryCategory, MemoryRecord, StoreStatus, TaskRecord, TaskStatus, TimelineEvent,
    TimelineEventKind,
};

use super::present::{MemoryCard, present_memory};
use crate::platform::MemorySettings;

// ── Styles ───────────────────────────────────────────────────────────

const S_TITLE: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const S_RULE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
const S_LABEL: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Blue)));
const S_VALUE: Style = Style::new().bold();
const S_MUTED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
const S_OK: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const S_WARN: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
const S_ERR: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Red)));
const S_ACCENT: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const S_BODY: Style = Style::new();
const S_TIP: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));

const S_CAT_CORRECTION: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Red)));
const S_CAT_USER: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Magenta)));
const S_CAT_INSIGHT: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const S_CAT_DISCOVERY: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Blue)));
const S_CAT_WORK: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const S_CAT_CONSOLIDATED: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));

/// Whether to emit ANSI styles (CLI TTY only by default).
#[derive(Debug, Clone, Copy)]
pub struct MemoryStyle {
    enabled: bool,
}

impl MemoryStyle {
    pub fn plain() -> Self {
        Self { enabled: false }
    }

    /// Enable colors when stdout is a terminal and `NO_COLOR` is unset.
    pub fn auto_stdout() -> Self {
        Self {
            enabled: std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal(),
        }
    }

    #[cfg(test)]
    pub fn forced(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn paint(self, style: Style, text: impl fmt::Display) -> String {
        if !self.enabled {
            return text.to_string();
        }
        format!("{}{}{}", style.render(), text, style.render_reset())
    }

    pub fn category(self, cat: MemoryCategory, text: impl fmt::Display) -> String {
        self.paint(category_style(cat), text)
    }
}

fn category_style(c: MemoryCategory) -> Style {
    match c {
        MemoryCategory::Correction => S_CAT_CORRECTION,
        MemoryCategory::User => S_CAT_USER,
        MemoryCategory::Insight => S_CAT_INSIGHT,
        MemoryCategory::Discovery => S_CAT_DISCOVERY,
        MemoryCategory::Work => S_CAT_WORK,
        MemoryCategory::Consolidated => S_CAT_CONSOLIDATED,
    }
}

// ── Shared helpers ───────────────────────────────────────────────────

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

pub fn parse_category_filter(raw: &str) -> Option<MemoryCategory> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "correction" | "corr" => Some(MemoryCategory::Correction),
        "insight" | "ins" => Some(MemoryCategory::Insight),
        "user" | "pref" | "preference" => Some(MemoryCategory::User),
        "consolidated" | "merge" | "merged" => Some(MemoryCategory::Consolidated),
        "discovery" | "map" | "layout" => Some(MemoryCategory::Discovery),
        "work" | "change" | "changes" => Some(MemoryCategory::Work),
        _ => None,
    }
}

pub fn category_help_list() -> &'static str {
    "correction | user | insight | discovery | work | consolidated"
}

fn category_title(c: MemoryCategory) -> &'static str {
    match c {
        MemoryCategory::Correction => "Correction",
        MemoryCategory::User => "User preference",
        MemoryCategory::Insight => "Insight",
        MemoryCategory::Discovery => "Project map",
        MemoryCategory::Work => "Work / change",
        MemoryCategory::Consolidated => "Consolidated",
    }
}

fn rule(out: &mut String, sty: MemoryStyle, title: &str) {
    use std::fmt::Write;
    let _ = writeln!(out, "{}", sty.paint(S_TITLE, title));
    let bar = "─".repeat(title.chars().count().clamp(12, 48));
    let _ = writeln!(out, "{}", sty.paint(S_RULE, bar));
}

fn kv(out: &mut String, sty: MemoryStyle, key: &str, value: impl fmt::Display) {
    use std::fmt::Write;
    let _ = writeln!(
        out,
        "  {} {}",
        sty.paint(S_LABEL, format!("{key:<18}")),
        sty.paint(S_VALUE, value)
    );
}

fn on_off(sty: MemoryStyle, v: bool) -> String {
    if v {
        sty.paint(S_OK, "on")
    } else {
        sty.paint(S_MUTED, "off")
    }
}

// ── Writers ──────────────────────────────────────────────────────────

/// Overview: store stats + auto-memory settings.
pub fn write_status(out: &mut String, status: &StoreStatus, settings: Option<&MemorySettings>, sty: MemoryStyle) {
    use std::fmt::Write;
    rule(out, sty, "Memory store");
    let _ = writeln!(out);
    kv(out, sty, "Entries", status.total_memories);
    kv(
        out,
        sty,
        "Tasks",
        format!("{} completed / {} total", status.completed_tasks, status.total_tasks),
    );
    let avg = if status.avg_task_score.is_finite() && status.total_tasks > 0 {
        format!("{:.2}", status.avg_task_score)
    } else {
        "—".into()
    };
    kv(out, sty, "Average task score", avg);
    if status.pending_embeddings > 0 {
        let _ = writeln!(
            out,
            "  {} {}",
            sty.paint(S_WARN, format!("{:<18}", "Embeddings pending")),
            sty.paint(
                S_WARN,
                format!("{}  (search may be incomplete until embedded)", status.pending_embeddings)
            )
        );
    }

    if let Some(s) = settings {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", sty.paint(S_TITLE, "Automatic features"));
        kv(out, sty, "enabled", on_off(sty, s.enabled));
        kv(out, sty, "auto recall", on_off(sty, s.auto_recall));
        kv(out, sty, "capture work", on_off(sty, s.auto_capture_work));
        kv(out, sty, "capture exploration", on_off(sty, s.auto_capture_exploration));
        kv(out, sty, "top-k", s.top_k);
        kv(out, sty, "context budget", format!("{} chars", s.context_budget_chars));
        kv(out, sty, "min query length", s.min_query_length);
    }

    if !status.categories.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", sty.paint(S_TITLE, "By category"));
        let mut cats = status.categories.clone();
        cats.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| category_str(a.category).cmp(category_str(b.category)))
        });
        for c in &cats {
            let name = sty.category(c.category, format!("{:<14}", category_str(c.category)));
            let count = sty.paint(S_VALUE, format!("{:>4}", c.count));
            let title = sty.paint(S_MUTED, category_title(c.category));
            let _ = writeln!(out, "  {name} {count}  {title}");
        }
    }

    if !status.top_memories.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", sty.paint(S_TITLE, "Most useful (by weight)"));
        for (i, m) in status.top_memories.iter().enumerate() {
            // TopMemory has no category field — present as insight-like prose.
            let card = present_memory(MemoryCategory::Insight, &m.content);
            let _ = writeln!(
                out,
                "  {}. {}  {}",
                sty.paint(S_ACCENT, i + 1),
                sty.paint(S_MUTED, format!("w={:.2}", m.weight)),
                sty.paint(S_BODY, &card.headline),
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{}",
        sty.paint(S_TIP, "Tip: memory recent · memory search <q> · memory list work")
    );
    let _ = writeln!(out, "{}", sty.paint(S_TIP, "     (slash: /memory … · CLI: elph memory …)"));
}

pub fn write_memories(out: &mut String, records: &[MemoryRecord], filter: Option<MemoryCategory>, sty: MemoryStyle) {
    use std::fmt::Write;
    if records.is_empty() {
        let label = filter.map(category_title).unwrap_or("matching");
        let _ = writeln!(out, "{}", sty.paint(S_MUTED, format!("No {label} memories found.")));
        let _ = writeln!(out, "{}", sty.paint(S_TIP, format!("Categories: {}", category_help_list())));
        return;
    }

    let title = match filter {
        Some(c) => format!("{} · {}", records.len(), category_title(c)),
        None => format!("{} memories", records.len()),
    };
    rule(out, sty, &title);
    let _ = writeln!(out);

    for (i, r) in records.iter().enumerate() {
        write_memory_card(
            out,
            sty,
            i + 1,
            r.category,
            &present_memory(r.category, &r.content),
            MemoryMeta {
                weight: Some(r.weight),
                used: Some(r.retrieval_count),
                when: Some(time_ago(r.created_at)),
                embed: Some(r.embedding_status),
                score: None,
            },
        );
        if i + 1 < records.len() {
            let _ = writeln!(out);
        }
    }
}

struct MemoryMeta {
    weight: Option<f64>,
    used: Option<u32>,
    when: Option<String>,
    embed: Option<EmbeddingStatus>,
    score: Option<f64>,
}

fn write_memory_card(
    out: &mut String,
    sty: MemoryStyle,
    index: usize,
    category: MemoryCategory,
    card: &MemoryCard,
    meta: MemoryMeta,
) {
    use std::fmt::Write;

    // Headline first — the only line that must be scannable.
    let _ = writeln!(
        out,
        "{}  {}",
        sty.paint(S_ACCENT, format!("{index}.")),
        sty.paint(S_BODY, &card.headline),
    );

    // Category + sparse meta on one muted line (skip defaults / noise).
    let mut bits: Vec<String> = Vec::new();
    bits.push(sty.category(category, category_title(category)));
    if let Some(s) = meta.score {
        bits.push(sty.paint(S_MUTED, format!("match {:.0}%", s * 100.0)));
    }
    if let Some(w) = meta.weight {
        // Default weight ≈ 1.0 is noise for list scanning.
        if (w - 1.0).abs() > 0.05 {
            bits.push(sty.paint(S_MUTED, format!("weight {w:.1}")));
        }
    }
    if let Some(u) = meta.used
        && u > 0
    {
        bits.push(sty.paint(S_MUTED, format!("used {u}×")));
    }
    if let Some(when) = &meta.when {
        bits.push(sty.paint(S_MUTED, when));
    }
    if let Some(emb) = meta.embed {
        match emb {
            EmbeddingStatus::Ok => {}
            EmbeddingStatus::Pending => bits.push(sty.paint(S_WARN, "embed pending")),
            EmbeddingStatus::Truncated => bits.push(sty.paint(S_WARN, "embed truncated")),
        }
    }
    let _ = writeln!(out, "   {}", bits.join(" · "));

    for d in &card.details {
        let _ = writeln!(out, "   {} {}", sty.paint(S_MUTED, "·"), sty.paint(S_BODY, d));
    }
}

pub fn write_tasks(out: &mut String, tasks: &[TaskRecord], sty: MemoryStyle) {
    use std::fmt::Write;
    if tasks.is_empty() {
        let _ = writeln!(out, "{}", sty.paint(S_MUTED, "No memory tasks recorded yet."));
        let _ = writeln!(
            out,
            "{}",
            sty.paint(S_TIP, "Tasks are created automatically on each substantive agent turn.")
        );
        return;
    }

    rule(out, sty, &format!("Recent tasks ({})", tasks.len()));
    let _ = writeln!(out);

    for (i, t) in tasks.iter().enumerate() {
        let (status_label, status_style) = match t.status {
            TaskStatus::InProgress => ("in progress", S_WARN),
            TaskStatus::Completed => ("done", S_OK),
            TaskStatus::Failed => ("failed", S_ERR),
        };
        let when = t.started_at.map(time_ago).unwrap_or_else(|| "—".into());
        let desc = first_line(t.description.as_deref().unwrap_or("(no description)"), 90);

        let _ = writeln!(
            out,
            "{}  {}",
            sty.paint(S_ACCENT, format!("{}.", i + 1)),
            sty.paint(S_BODY, desc),
        );

        let mut meta_bits: Vec<String> = Vec::new();
        meta_bits.push(sty.paint(status_style, status_label));
        if let Some(s) = t.task_score {
            meta_bits.push(sty.paint(S_MUTED, format!("score {s:.2}")));
        }
        meta_bits.push(sty.paint(S_MUTED, when));
        let _ = writeln!(out, "   {}", meta_bits.join(" · "));

        // Only show operational stats when they carry signal.
        let tokens = t.tokens_used.unwrap_or(0);
        let calls = t.tool_calls.unwrap_or(0);
        let errors = t.errors.unwrap_or(0);
        let corr = t.user_corrections.unwrap_or(0);
        if tokens > 0 || calls > 0 || errors > 0 || corr > 0 {
            let mut stats = Vec::new();
            if tokens > 0 {
                stats.push(format!("{tokens} tokens"));
            }
            if calls > 0 {
                stats.push(format!("{calls} tools"));
            }
            if errors > 0 {
                stats.push(format!("{errors} errors"));
            }
            if corr > 0 {
                stats.push(format!("{corr} user fixes"));
            }
            let _ = writeln!(out, "   {}", sty.paint(S_MUTED, stats.join(" · ")));
        }

        if !t.retrievals.is_empty() {
            let _ = writeln!(out, "   {}", sty.paint(S_MUTED, "recalled"));
            for r in t.retrievals.iter().take(4) {
                let sim = r.similarity.unwrap_or(0.0);
                let preview = first_line(&r.preview, 52);
                let rated = r.self_report.map(|s| format!(" · rated {s}/3")).unwrap_or_default();
                let _ = writeln!(
                    out,
                    "   {} {}  {}",
                    sty.paint(S_MUTED, "·"),
                    sty.category(r.category, category_str(r.category)),
                    sty.paint(S_MUTED, format!("{preview}  ({:.0}%{rated})", sim * 100.0)),
                );
            }
        }

        if !t.created_memories.is_empty() {
            let _ = writeln!(out, "   {}", sty.paint(S_MUTED, "stored"));
            for c in t.created_memories.iter().take(4) {
                let _ = writeln!(
                    out,
                    "   {} {}  {}",
                    sty.paint(S_OK, "+"),
                    sty.category(c.category, category_str(c.category)),
                    sty.paint(S_BODY, first_line(&c.preview, 56)),
                );
            }
        }

        if i + 1 < tasks.len() {
            let _ = writeln!(out);
        }
    }
}

pub fn write_timeline(out: &mut String, events: &[TimelineEvent], sty: MemoryStyle) {
    use std::fmt::Write;
    if events.is_empty() {
        let _ = writeln!(out, "{}", sty.paint(S_MUTED, "Timeline is empty."));
        return;
    }

    rule(out, sty, &format!("Timeline ({} events)", events.len()));
    let _ = writeln!(out);
    for e in events {
        let when = time_ago(e.timestamp);
        let (kind, kind_style) = match e.kind {
            TimelineEventKind::Task => ("task", S_WARN),
            TimelineEventKind::Memory => ("mem ", S_ACCENT),
        };
        let summary = first_line(&e.summary, 70);
        let _ = writeln!(
            out,
            "  {}  {}  {}",
            sty.paint(S_MUTED, format!("{when:>8}")),
            sty.paint(kind_style, format!("{kind:<4}")),
            sty.paint(S_BODY, summary),
        );
    }
}

pub fn write_search_results(out: &mut String, query: &str, memories: &[Memory], sty: MemoryStyle) {
    use std::fmt::Write;
    let meaningful: Vec<&Memory> = memories.iter().filter(|m| m.score >= 0.12).collect();
    let weak_only = meaningful.is_empty() && !memories.is_empty();

    if memories.is_empty() {
        let _ = writeln!(out, "{}", sty.paint(S_MUTED, format!("No memories matched “{query}”.")));
        let _ = writeln!(
            out,
            "{}",
            sty.paint(S_TIP, "Try a shorter phrase, or: memory recent · memory list work")
        );
        return;
    }

    if weak_only {
        rule(out, sty, &format!("Search · weak matches for “{query}”"));
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{}",
            sty.paint(
                S_WARN,
                format!(
                    "No strong semantic matches (best score {:.0}%).",
                    memories.iter().map(|m| m.score).fold(0.0_f64, f64::max) * 100.0
                )
            )
        );
        let _ = writeln!(
            out,
            "{}",
            sty.paint(
                S_MUTED,
                "Embeddings may still be pending — try again after a session, or run consolidate."
            )
        );
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{}",
            sty.paint(S_TIP, "Showing top rows by retrieval order (not reliable ranking):")
        );
        let _ = writeln!(out);
    } else {
        rule(out, sty, &format!("Search · {} result(s) for “{query}”", meaningful.len()));
        let _ = writeln!(out);
    }

    let show: Vec<&Memory> = if weak_only {
        memories.iter().take(5).collect()
    } else {
        meaningful
    };

    for (i, m) in show.iter().enumerate() {
        write_memory_card(
            out,
            sty,
            i + 1,
            m.category,
            &present_memory(m.category, &m.content),
            MemoryMeta {
                weight: Some(m.weight),
                used: Some(m.retrieval_count),
                when: None,
                embed: None,
                score: Some(m.score),
            },
        );
        if i + 1 < show.len() {
            let _ = writeln!(out);
        }
    }
}

pub fn write_purge(out: &mut String, count: u32, threshold: f64, sty: MemoryStyle) {
    use std::fmt::Write;
    if count == 0 {
        let _ = writeln!(
            out,
            "{}",
            sty.paint(S_MUTED, format!("Nothing to purge (no memories below weight {threshold:.2})."))
        );
    } else {
        let _ = writeln!(
            out,
            "{}",
            sty.paint(
                S_OK,
                format!(
                    "Purged {count} weak memor{} (weight < {threshold:.2}).",
                    if count == 1 { "y" } else { "ies" }
                )
            )
        );
    }
}

pub fn write_flush(out: &mut String, memories: u32, tasks: u32, sty: MemoryStyle) {
    use std::fmt::Write;
    if memories == 0 && tasks == 0 {
        let _ = writeln!(out, "{}", sty.paint(S_MUTED, "Memory store was already empty."));
        return;
    }
    let _ = writeln!(
        out,
        "{}",
        sty.paint(
            S_OK,
            format!(
                "Flushed memory store: deleted {memories} memor{}, {tasks} task{}.",
                if memories == 1 { "y" } else { "ies" },
                if tasks == 1 { "" } else { "s" },
            )
        )
    );
    let _ = writeln!(
        out,
        "{}",
        sty.paint(S_MUTED, "All lessons, work logs, and recall tasks are gone.")
    );
}

pub fn write_flush_cancelled(out: &mut String, sty: MemoryStyle) {
    use std::fmt::Write;
    let _ = writeln!(out, "{}", sty.paint(S_MUTED, "Flush cancelled — nothing deleted."));
}

pub fn write_consolidate(out: &mut String, merged: u32, deleted: u32, sty: MemoryStyle) {
    use std::fmt::Write;
    if merged == 0 {
        let _ = writeln!(out, "{}", sty.paint(S_MUTED, "No near-duplicate memories to consolidate."));
    } else {
        let _ = writeln!(
            out,
            "{}",
            sty.paint(
                S_OK,
                format!(
                    "Consolidated {merged} pair{} into summary entries (removed {deleted} sources).",
                    if merged == 1 { "" } else { "s" }
                )
            )
        );
    }
}

pub fn write_help(out: &mut String, sty: MemoryStyle) {
    use std::fmt::Write;
    rule(out, sty, "Memory commands");
    let _ = writeln!(out);
    let cmd = |name: &str, desc: &str| -> String {
        format!("  {}  {}", sty.paint(S_ACCENT, format!("{name:<22}")), sty.paint(S_MUTED, desc))
    };
    let _ = writeln!(out, "{}", cmd("status", "Overview + auto-feature flags"));
    let _ = writeln!(
        out,
        "{}",
        cmd("list [category] [n]", "List memories (optional category & limit)")
    );
    let _ = writeln!(out, "{}", cmd("recent [n] [category]", "Newest entries first (default 10)"));
    let _ = writeln!(out, "{}", cmd("tasks [n]", "Recent recall tasks (default 10)"));
    let _ = writeln!(out, "{}", cmd("log [n]", "Timeline of tasks & memories (default 20)"));
    let _ = writeln!(out, "{}", cmd("search <query>", "Semantic search (does not train a task)"));
    let _ = writeln!(out, "{}", cmd("purge [threshold]", "Delete weak memories (default weight 0.5)"));
    let _ = writeln!(out, "{}", cmd("flush", "Wipe entire store (confirm required)"));
    let _ = writeln!(out, "{}", cmd("consolidate", "Merge near-duplicate entries (maintenance)"));
    let _ = writeln!(out, "{}", cmd("help", "This message"));
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{} {}",
        sty.paint(S_LABEL, "Categories:"),
        sty.paint(S_MUTED, category_help_list())
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", sty.paint(S_TITLE, "Automatic behavior (settings.memory.*)"));
    let _ = writeln!(
        out,
        "{}",
        sty.paint(S_MUTED, "  enabled, autoRecall, autoCaptureWork, autoCaptureExploration")
    );
    let _ = writeln!(
        out,
        "{}",
        sty.paint(S_MUTED, "  topK, contextBudgetChars, minQueryLength")
    );
    let _ = writeln!(
        out,
        "{}",
        sty.paint(S_MUTED, "Embed model: settings.models.embed (model, quantized)")
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", sty.paint(S_TITLE, "Examples"));
    for ex in [
        "/memory status",
        "/memory recent 5 work",
        "/memory search auth middleware",
        "elph memory list discovery --limit 20",
    ] {
        let _ = writeln!(out, "  {}", sty.paint(S_ACCENT, ex));
    }
}

pub fn write_note(out: &mut String, text: &str, sty: MemoryStyle) {
    use std::fmt::Write;
    let _ = writeln!(out, "{}", sty.paint(S_WARN, text));
}

fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or(s).trim();
    // Prefer human presentation for previews that look like stored memory text.
    let card = present_memory(MemoryCategory::Insight, line);
    let text = card.headline;
    if text.chars().count() <= max {
        text
    } else {
        let truncated: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

#[allow(dead_code)] // kept for local helpers / future detail truncation
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_category_filter_accepts_aliases() {
        assert_eq!(parse_category_filter("user"), Some(MemoryCategory::User));
        assert_eq!(parse_category_filter("WORK"), Some(MemoryCategory::Work));
        assert_eq!(parse_category_filter("map"), Some(MemoryCategory::Discovery));
        assert_eq!(parse_category_filter("nope"), None);
    }

    #[test]
    fn truncate_shortens_long_strings() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hell…");
    }

    #[test]
    fn help_mentions_recent_and_consolidate() {
        let mut buf = String::new();
        write_help(&mut buf, MemoryStyle::plain());
        assert!(buf.contains("recent"));
        assert!(buf.contains("consolidate"));
        assert!(buf.contains("flush"));
        assert!(buf.contains("autoRecall"));
    }

    #[test]
    fn flush_summary_is_clear() {
        let mut buf = String::new();
        write_flush(&mut buf, 3, 1, MemoryStyle::plain());
        assert!(buf.contains("Flushed"));
        assert!(buf.contains("3 memories"));
        assert!(buf.contains("1 task"));
        let mut empty = String::new();
        write_flush(&mut empty, 0, 0, MemoryStyle::plain());
        assert!(empty.contains("already empty"));
    }

    #[test]
    fn plain_style_has_no_ansi() {
        let mut buf = String::new();
        write_purge(&mut buf, 2, 0.5, MemoryStyle::plain());
        assert!(!buf.contains('\u{1b}'));
        assert!(buf.contains("Purged 2"));
    }

    #[test]
    fn forced_color_emits_ansi() {
        let mut buf = String::new();
        write_purge(&mut buf, 2, 0.5, MemoryStyle::forced(true));
        assert!(buf.contains('\u{1b}'), "expected ANSI escapes: {buf:?}");
    }

    #[test]
    fn list_hides_json_tool_dumps() {
        let rec = MemoryRecord {
            id: "abc".into(),
            content: r#"Tool `edit_file` failed with args: {"path":"src/x.rs","new_string":"HUGE"}
Failed approach: Tool execution error: edit_file
Working approach: unknown"#
                .into(),
            category: MemoryCategory::Correction,
            weight: 1.5,
            retrieval_count: 0,
            created_at: 0,
            embedding_status: EmbeddingStatus::Ok,
        };
        let mut buf = String::new();
        write_memories(&mut buf, &[rec], None, MemoryStyle::plain());
        assert!(buf.contains("edit_file"), "{buf}");
        assert!(!buf.contains("new_string"), "{buf}");
        assert!(!buf.contains("HUGE"), "{buf}");
        assert!(buf.contains("src/x.rs") || buf.contains("path"), "{buf}");
        // No raw id line cluttering the list
        assert!(!buf.contains("id abc") && !buf.contains("#abc"), "{buf}");
        // Weight shown only when non-default
        assert!(buf.contains("weight 1.5"), "{buf}");
    }

    #[test]
    fn list_layout_is_scannable() {
        let rec = MemoryRecord {
            id: "019fb93d505ag31q".into(),
            content: r#"[consolidated] Tool `edit_file` failed with args: {"new_string":"let x = 1"}
---
Tool `update_goal` failed with args: {"status":"complete"}
Failed approach: Tool execution error: update_goal
Working approach: unknown"#
                .into(),
            category: MemoryCategory::Consolidated,
            weight: 1.0,
            retrieval_count: 0,
            created_at: 0,
            embedding_status: EmbeddingStatus::Ok,
        };
        let mut buf = String::new();
        write_memories(&mut buf, &[rec], None, MemoryStyle::plain());
        // Headline-first card
        let body = buf.lines().find(|l| l.starts_with("1.")).unwrap_or("");
        assert!(body.contains("Merged") || body.contains("edit_file"), "{buf}");
        assert!(!buf.contains("new_string"), "{buf}");
        assert!(!buf.contains("embed ready"), "{buf}");
        assert!(!buf.contains("019fb93d"), "{buf}");
        assert!(!buf.contains("weight 1.0"), "{buf}"); // default weight hidden
        assert!(buf.contains("Consolidated"), "{buf}");
    }
}
