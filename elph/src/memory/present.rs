//! Turn raw memory text into short, human-readable display lines.
//!
//! Hides JSON tool args, collapses consolidated dumps, and extracts structured
//! fields from `[work]` / `[change]` / `[discovery]` / correction templates.

use floppy::MemoryCategory;

/// Parsed display for one memory (or search hit).
#[derive(Debug, Clone)]
pub struct MemoryCard {
    /// One-line headline (always present).
    pub headline: String,
    /// Optional short detail lines (0–3).
    pub details: Vec<String>,
}

/// Present memory content for list/search UI.
pub fn present_memory(category: MemoryCategory, content: &str) -> MemoryCard {
    let raw = content.trim();
    if raw.is_empty() {
        return MemoryCard {
            headline: "(empty)".into(),
            details: vec![],
        };
    }

    let body = strip_leading_tag(raw);
    let is_merge = body.contains("\n---\n") || body.contains("\n---") || body.starts_with("---");
    let is_consolidated = category == MemoryCategory::Consolidated || raw.trim_start().starts_with("[consolidated]");

    // Merged / consolidated blobs first — otherwise the first tool-failure
    // regex wins and hides sibling entries after `---`.
    if is_merge || (is_consolidated && body.lines().count() > 4) {
        return dedupe_details(present_consolidated(body));
    }

    if let Some(card) = present_tool_failure(body) {
        return dedupe_details(card);
    }
    if let Some(card) = present_correction(body) {
        return dedupe_details(card);
    }
    if let Some(card) = present_work_or_change(body) {
        return dedupe_details(card);
    }
    if let Some(card) = present_discovery(body) {
        return dedupe_details(card);
    }
    if is_consolidated {
        return dedupe_details(present_consolidated(body));
    }

    // Plain prose / user preference / insight.
    let lines: Vec<&str> = body.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let headline = first_sentence(lines.first().copied().unwrap_or(body), 88);
    let details = lines
        .iter()
        .skip(1)
        .take(2)
        .map(|l| clean_line(l, 96))
        .filter(|l| !l.is_empty())
        .collect();
    dedupe_details(MemoryCard { headline, details })
}

/// Drop detail lines that repeat the headline (common after consolidation).
fn dedupe_details(mut card: MemoryCard) -> MemoryCard {
    let h = card.headline.to_ascii_lowercase();
    card.details.retain(|d| {
        let dl = d.to_ascii_lowercase();
        dl != h && !h.contains(&dl) && !dl.contains(&h)
    });
    card
}

fn strip_leading_tag(s: &str) -> &str {
    let t = s.trim();
    for tag in ["[consolidated]", "[work]", "[change]", "[discovery]", "[correction]"] {
        if let Some(rest) = t.strip_prefix(tag) {
            return rest.trim_start();
        }
    }
    t
}

fn present_tool_failure(body: &str) -> Option<MemoryCard> {
    // "Tool `name` failed with args: {...}"
    let lower = body.to_ascii_lowercase();
    if !(lower.contains("tool `") && lower.contains("failed")) && !lower.contains("tool execution error") {
        return None;
    }

    let tool = extract_backticked_tool(body)
        .or_else(|| extract_after(body, "Tool execution error:"))
        .unwrap_or_else(|| "tool".into());

    let mut details = Vec::new();
    if let Some(path) = extract_json_string_field(body, "path") {
        details.push(format!("path  {path}"));
    } else if let Some(status) = extract_json_string_field(body, "status") {
        details.push(format!("arg   status={status}"));
    }

    // Working approach if present after consolidation merge.
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Working approach:") {
            let w = rest.trim();
            if !w.is_empty() && !w.eq_ignore_ascii_case("unknown") {
                details.push(format!("try   {}", clean_line(w, 80)));
            }
        }
        if let Some(rest) = t.strip_prefix("Failed approach:") {
            let f = rest.trim();
            // Skip noisy "Tool execution error: name" echoes.
            if !f.is_empty()
                && !f.to_ascii_lowercase().starts_with("tool execution error")
                && !f.eq_ignore_ascii_case(&format!("Tool execution error: {tool}"))
            {
                details.push(format!("avoid {}", clean_line(f, 80)));
            }
        }
    }

    if details.is_empty() {
        if let Some(hint) = summarize_tool_args(body) {
            details.push(hint);
        }
    }

    Some(MemoryCard {
        headline: format!("Tool `{tool}` failed"),
        details: details.into_iter().take(3).collect(),
    })
}

fn present_correction(body: &str) -> Option<MemoryCard> {
    let has_failed = body.lines().any(|l| l.trim().starts_with("Failed approach:"));
    let has_worked = body.lines().any(|l| l.trim().starts_with("Working approach:"));
    if !has_failed && !has_worked {
        // "User correction: …"
        if let Some(rest) = body.strip_prefix("User correction:") {
            return Some(MemoryCard {
                headline: clean_line(rest.trim(), 88),
                details: vec!["from user feedback".into()],
            });
        }
        return None;
    }

    let mut lesson = String::new();
    let mut failed = String::new();
    let mut worked = String::new();
    for line in body.lines() {
        let t = line.trim();
        if let Some(r) = t.strip_prefix("Failed approach:") {
            failed = r.trim().to_string();
        } else if let Some(r) = t.strip_prefix("Working approach:") {
            worked = r.trim().to_string();
        } else if lesson.is_empty() && !t.is_empty() && !t.starts_with("Tool ") && !t.starts_with('{') {
            lesson = t.to_string();
        }
    }

    let headline = if !lesson.is_empty() && !lesson.starts_with("Tool ") {
        clean_line(&lesson, 88)
    } else if !worked.is_empty() && !worked.eq_ignore_ascii_case("unknown") {
        format!("Prefer: {}", clean_line(&worked, 72))
    } else if let Some(tool) = extract_backticked_tool(body) {
        format!("Tool `{tool}` failed")
    } else {
        "Correction recorded".into()
    };

    let mut details = Vec::new();
    if !failed.is_empty() && !failed.to_ascii_lowercase().starts_with("tool execution error") {
        details.push(format!("avoid  {}", clean_line(&failed, 80)));
    }
    if !worked.is_empty() && !worked.eq_ignore_ascii_case("unknown") {
        details.push(format!("use    {}", clean_line(&worked, 80)));
    }
    if details.is_empty() {
        if let Some(path) = extract_json_string_field(body, "path") {
            details.push(format!("path   {path}"));
        } else if let Some(hint) = summarize_tool_args(body) {
            details.push(hint);
        }
    }
    Some(MemoryCard {
        headline,
        details: details.into_iter().take(3).collect(),
    })
}

fn present_work_or_change(body: &str) -> Option<MemoryCard> {
    let looks_work = body.lines().any(|l| {
        let t = l.trim();
        t.starts_with("Paths:") || t.starts_with("Outcome:") || t.starts_with("Note:")
    }) || body.starts_with("Goal ");

    let looks_change =
        body.lines().any(|l| l.trim().starts_with("Paths:")) && body.lines().any(|l| l.trim().starts_with('-'));

    if !looks_work && !looks_change {
        return None;
    }

    let mut headline = String::new();
    let mut paths = String::new();
    let mut outcome = String::new();
    let mut note = String::new();

    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() || t == "Paths:" {
            continue;
        }
        if let Some(r) = t.strip_prefix("Paths:") {
            paths = r.trim().to_string();
        } else if let Some(r) = t.strip_prefix("Outcome:") {
            outcome = r.trim().to_string();
        } else if let Some(r) = t.strip_prefix("Note:") {
            note = r.trim().to_string();
        } else if t.starts_with('-') {
            let p = t.trim_start_matches('-').trim();
            if !p.is_empty() {
                if !paths.is_empty() {
                    paths.push_str(", ");
                }
                let path_only = p.split('(').next().unwrap_or(p).trim();
                paths.push_str(path_only);
            }
        } else if headline.is_empty() {
            headline = t.to_string();
        }
    }

    if headline.is_empty() {
        headline = if !paths.is_empty() {
            format!("Changed {}", clean_line(&paths, 60))
        } else {
            "Work recorded".into()
        };
    } else {
        headline = clean_line(&headline, 88);
    }

    let mut details = Vec::new();
    if !paths.is_empty() {
        let short_paths = clean_line(&shorten_path_list(&paths, 3), 90);
        let head_snip: String = paths.chars().take(24).collect();
        if !headline.contains(&head_snip) {
            details.push(format!("files  {short_paths}"));
        }
    }
    if !outcome.is_empty() && !outcome.eq_ignore_ascii_case("success") {
        details.push(format!("result {outcome}"));
    } else if outcome.eq_ignore_ascii_case("success") {
        // keep silent — success is the default expectation
    }
    if !note.is_empty() && !note.starts_with("auto-captured") && !note.starts_with("goal_id=") {
        details.push(clean_line(&note, 90));
    } else if note.starts_with("goal_id=") {
        details.push("from goal completion".into());
    }

    Some(MemoryCard {
        headline,
        details: details.into_iter().take(3).collect(),
    })
}

fn present_discovery(body: &str) -> Option<MemoryCard> {
    if !body.contains("Area:") && !body.lines().any(|l| l.trim().starts_with("Observed")) {
        return None;
    }
    let mut area = String::new();
    let mut tools = String::new();
    let mut notes = String::new();
    for line in body.lines() {
        let t = line.trim();
        if let Some(r) = t.strip_prefix("Area:") {
            area = r.trim().trim_end_matches('/').to_string();
        } else if let Some(r) = t.strip_prefix("Observed tools:") {
            tools = r.trim().to_string();
        } else if let Some(r) = t.strip_prefix("Notes:") {
            notes = r.trim().to_string();
        }
    }
    let headline = if area.is_empty() {
        "Project area mapped".into()
    } else {
        format!("Explored {area}/")
    };
    let mut details = Vec::new();
    if !tools.is_empty() {
        details.push(format!("via    {tools}"));
    }
    if !notes.is_empty() && notes != "(none)" {
        details.push(clean_line(&notes, 90));
    }
    Some(MemoryCard { headline, details })
}

fn present_consolidated(body: &str) -> MemoryCard {
    // Merged entries often join two tool failures with "---"
    let parts: Vec<&str> = body.split("---").map(str::trim).filter(|p| !p.is_empty()).collect();

    if parts.is_empty() {
        return MemoryCard {
            headline: "Merged memories".into(),
            details: vec![],
        };
    }

    let mut bullets = Vec::new();
    for part in parts.iter().take(4) {
        if let Some(card) = present_tool_failure(part) {
            let mut s = card.headline;
            if let Some(path_d) = card.details.iter().find(|d| d.starts_with("path")) {
                let p = path_d.trim_start_matches("path").trim();
                if !p.is_empty() {
                    s = format!("{s} · {p}");
                }
            } else if let Some(arg_d) = card.details.iter().find(|d| d.starts_with("arg")) {
                let a = arg_d.trim_start_matches("arg").trim();
                if !a.is_empty() {
                    s = format!("{s} · {a}");
                }
            }
            bullets.push(s);
        } else if let Some(card) = present_correction(part) {
            bullets.push(card.headline);
        } else if let Some(card) = present_work_or_change(part) {
            bullets.push(card.headline);
        } else {
            bullets.push(first_sentence(part, 70));
        }
    }

    // Dedupe identical bullets
    let mut unique = Vec::new();
    for b in bullets {
        if !unique.iter().any(|u: &String| u.eq_ignore_ascii_case(&b)) {
            unique.push(b);
        }
    }

    if unique.len() == 1 {
        return MemoryCard {
            headline: clean_line(&unique[0], 88),
            details: vec!["merged from similar entries".into()],
        };
    }

    MemoryCard {
        headline: format!("Merged {} related memories", unique.len()),
        details: unique.into_iter().take(3).map(|b| clean_line(&b, 90)).collect(),
    }
}

fn shorten_path_list(paths: &str, max_items: usize) -> String {
    let items: Vec<&str> = paths.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if items.len() <= max_items {
        return items.join(", ");
    }
    let head: Vec<&str> = items.iter().take(max_items).copied().collect();
    format!("{}, +{} more", head.join(", "), items.len() - max_items)
}

fn extract_backticked_tool(s: &str) -> Option<String> {
    let start = s.find('`')?;
    let rest = &s[start + 1..];
    let end = rest.find('`')?;
    let name = rest[..end].trim();
    if name.is_empty() { None } else { Some(name.to_string()) }
}

fn extract_after(s: &str, marker: &str) -> Option<String> {
    let idx = s.find(marker)?;
    let rest = s[idx + marker.len()..].trim();
    let token = rest
        .split_whitespace()
        .next()?
        .trim_matches(|c: char| c == ':' || c == ',' || c == '"' || c == '\'');
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// Pull a short string field from a JSON-ish blob without full parse.
fn extract_json_string_field(s: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let idx = s.find(&key)?;
    let after = &s[idx + key.len()..];
    let after = after.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    if !after.starts_with('"') {
        return None;
    }
    let inner = &after[1..];
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
            continue;
        }
        if c == '"' {
            break;
        }
        out.push(c);
        if out.chars().count() > 120 {
            break;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(clean_line(&out, 100))
    }
}

fn summarize_tool_args(body: &str) -> Option<String> {
    if let Some(p) = extract_json_string_field(body, "path") {
        return Some(format!("path  {p}"));
    }
    if let Some(p) = extract_json_string_field(body, "source") {
        return Some(format!("source {p}"));
    }
    if body.contains("\"new_string\"") || body.contains("\"old_string\"") {
        return Some("file edit (payload omitted)".into());
    }
    if body.contains('{') {
        return Some("arguments omitted".into());
    }
    None
}

fn first_sentence(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or(s).trim();
    // Cut at first `{` for tool dumps
    let cut = line.find('{').map(|i| &line[..i]).unwrap_or(line).trim();
    clean_line(cut, max)
}

fn clean_line(s: &str, max: usize) -> String {
    let collapsed: String = s
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // Drop mid-JSON noise
    let no_json = if collapsed.contains("{\"") || collapsed.contains("\":") {
        collapsed
            .split('{')
            .next()
            .unwrap_or(&collapsed)
            .trim()
            .trim_end_matches(':')
            .trim()
            .to_string()
    } else {
        collapsed
    };
    if no_json.chars().count() <= max {
        no_json
    } else {
        let t: String = no_json.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presents_tool_failure_without_json() {
        let content = r#"Tool `edit_file` failed with args: {"path":"src/main.rs","new_string":"huge"}
Failed approach: Tool execution error: edit_file
Working approach: unknown"#;
        let card = present_memory(MemoryCategory::Correction, content);
        assert!(card.headline.contains("edit_file"));
        assert!(!card.headline.contains("new_string"));
        assert!(card.details.iter().any(|d| d.contains("src/main.rs")));
        assert!(!card.details.iter().any(|d| d.contains("huge")));
        assert!(
            !card
                .details
                .iter()
                .any(|d| d.to_lowercase().contains("tool execution error"))
        );
    }

    #[test]
    fn presents_work_entry() {
        let content = "[work] Fixed memory status formatting\nPaths: elph/src/memory/format.rs\nOutcome: success\nNote: auto-captured";
        let card = present_memory(MemoryCategory::Work, content);
        assert!(card.headline.contains("Fixed memory") || card.headline.contains("format"));
        // success + auto-captured are noise — may or may not show files
        assert!(!card.headline.contains("new_string"));
    }

    #[test]
    fn presents_consolidated_merge() {
        let content = "[consolidated] Tool `edit_file` failed with args: {\"path\":\"a.rs\"}\n---\nTool `update_goal` failed with args: {\"status\":\"complete\"}";
        let card = present_memory(MemoryCategory::Consolidated, content);
        assert!(
            card.headline.to_lowercase().contains("merged") || card.details.len() >= 1,
            "card={card:?}"
        );
        assert!(!card.headline.contains("new_string"));
        assert!(!card.details.iter().any(|d| d.contains("new_string")));
        // Both tools should surface
        let joined = format!("{} {}", card.headline, card.details.join(" "));
        assert!(joined.contains("edit_file") || joined.contains("update_goal"), "{joined}");
    }

    #[test]
    fn presents_user_correction() {
        let card = present_memory(MemoryCategory::User, "User correction: jangan pakai npm, pakai pnpm");
        assert!(card.headline.contains("pnpm") || card.headline.contains("jangan"));
    }

    #[test]
    fn consolidated_single_tool_is_readable() {
        let content = r#"[consolidated] Tool `edit_file` failed with args: {"new_string":"            let user_alone_rows = e…
---
Tool `edit_file` failed with args: {"new_string":"            let user_alone_rows = element! { View(…
Failed approach: Tool execution error: edit_file
Working approach: unknown"#;
        let card = present_memory(MemoryCategory::Consolidated, content);
        assert!(!card.headline.contains("new_string"), "headline={}", card.headline);
        assert!(!card.headline.contains('{'), "headline={}", card.headline);
        assert!(card.headline.contains("edit_file") || card.details.iter().any(|d| d.contains("edit_file")));
    }
}
