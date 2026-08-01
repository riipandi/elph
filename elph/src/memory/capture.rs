//! Structured work/change/discovery memory templates and path helpers.

use std::collections::HashMap;
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// Max path entries recorded per turn (coalesced into one change memory).
pub const MAX_PATHS_PER_TURN: usize = 20;

/// `list_dir` calls on the same root before auto-writing a discovery.
pub const EXPLORATION_LIST_DIR_THRESHOLD: u32 = 2;

/// Reads under the same top-level prefix before auto-writing a discovery.
pub const EXPLORATION_READ_THRESHOLD: u32 = 3;

/// Max basename notes in a discovery entry.
pub const MAX_DISCOVERY_NOTES: usize = 8;

/// Mutation tools that leave a durable work footprint when successful.
pub const MUTATION_TOOLS: &[&str] = &["edit_file", "write_file", "delete_path", "move_path", "copy_path"];

/// True when the path likely holds secrets and must not be journaled in detail.
pub fn is_sensitive_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    let file_name = lower.rsplit(['/', '\\']).next().unwrap_or(lower.as_str());

    if file_name == ".env" || file_name.starts_with(".env.") {
        return true;
    }
    if file_name.ends_with(".pem")
        || file_name.ends_with(".p12")
        || file_name.ends_with(".key")
        || file_name.ends_with(".keystore")
    {
        return true;
    }
    if file_name == "id_rsa" || file_name == "id_ed25519" || file_name == "auth.json" {
        return true;
    }

    const NEEDLES: &[&str] = &["credentials", "secret", "secrets", "/token", "\\token", ".token"];
    NEEDLES.iter().any(|n| lower.contains(n))
}

/// Extract path-like strings from mutation tool args.
pub fn paths_from_tool_input(tool_name: &str, input: &Value) -> Vec<String> {
    let mut out = Vec::new();
    match tool_name {
        "edit_file" | "write_file" | "delete_path" | "read_file" | "list_dir" => {
            if let Some(p) = input.get("path").and_then(Value::as_str) {
                out.push(p.to_string());
            }
        }
        "move_path" | "copy_path" => {
            for key in ["source", "from", "path", "destination", "to", "dest"] {
                if let Some(p) = input.get(key).and_then(Value::as_str) {
                    out.push(p.to_string());
                }
            }
        }
        _ => {
            if let Some(p) = input.get("path").and_then(Value::as_str) {
                out.push(p.to_string());
            }
        }
    }
    out
}

pub fn is_mutation_tool(tool_name: &str) -> bool {
    MUTATION_TOOLS.contains(&tool_name)
}

pub fn is_exploration_tool(tool_name: &str) -> bool {
    matches!(tool_name, "list_dir" | "find_path" | "read_file" | "grep")
}

/// Normalize a path string to a stable area key (directory or top-level prefix).
pub fn area_key_from_path(path: &str) -> String {
    let p = path.trim().trim_end_matches(['/', '\\']);
    if p.is_empty() || p == "." {
        return ".".into();
    }
    let path = Path::new(p);
    // Prefer parent dir for file-like paths with an extension.
    let dir = if path.extension().is_some() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let s = dir.to_string_lossy().replace('\\', "/");
    if s.is_empty() {
        ".".into()
    } else {
        s.trim_end_matches('/').to_string()
    }
}

/// Top-level prefix for read aggregation (`elph/src/foo.rs` → `elph`).
pub fn top_level_prefix(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/");
    let stripped = normalized.trim_start_matches("./");
    stripped
        .split('/')
        .find(|s| !s.is_empty() && *s != ".")
        .unwrap_or(".")
        .to_string()
}

pub fn basename_note(path: &str) -> Option<String> {
    if is_sensitive_path(path) {
        return None;
    }
    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())?;
    // Skip noisy names
    if name == "." || name == ".." {
        return None;
    }
    Some(name.to_string())
}

/// Exploration counters + notes (subset of turn scratch used by capture helpers).
#[derive(Debug, Clone, Default)]
pub struct ExplorationScratch {
    pub list_dir_roots: HashMap<String, u32>,
    pub read_prefixes: HashMap<String, u32>,
    pub basename_notes: Vec<(String, String)>,
}

/// Update exploration counters from a successful tool call.
pub fn record_exploration(scratch: &mut ExplorationScratch, tool_name: &str, input: &Value) {
    if !is_exploration_tool(tool_name) {
        return;
    }
    match tool_name {
        "list_dir" => {
            let path = input.get("path").and_then(Value::as_str).unwrap_or(".");
            if is_sensitive_path(path) {
                return;
            }
            let key = area_key_from_path(path);
            *scratch.list_dir_roots.entry(key).or_insert(0) += 1;
        }
        "find_path" => {
            let path = input.get("path").and_then(Value::as_str).unwrap_or(".");
            if is_sensitive_path(path) {
                return;
            }
            let key = area_key_from_path(path);
            *scratch.list_dir_roots.entry(key).or_insert(0) += 1;
        }
        "read_file" => {
            if let Some(path) = input.get("path").and_then(Value::as_str) {
                if is_sensitive_path(path) {
                    return;
                }
                let prefix = top_level_prefix(path);
                *scratch.read_prefixes.entry(prefix).or_insert(0) += 1;
                if let Some(note) = basename_note(path)
                    && scratch.basename_notes.len() < MAX_DISCOVERY_NOTES
                    && !scratch.basename_notes.iter().any(|(n, _)| n == &note)
                {
                    scratch.basename_notes.push((note, "read_file".into()));
                }
            }
        }
        "grep" => {
            // Prefer path/glob scope when present.
            if let Some(path) = input.get("path").or_else(|| input.get("glob")).and_then(Value::as_str) {
                if is_sensitive_path(path) {
                    return;
                }
                let prefix = top_level_prefix(path);
                *scratch.read_prefixes.entry(prefix).or_insert(0) += 1;
            }
        }
        _ => {}
    }
}

/// Build discovery entries for areas that crossed exploration thresholds.
pub fn build_discovery_entries(
    list_dir_roots: &HashMap<String, u32>,
    read_prefixes: &HashMap<String, u32>,
    basename_notes: &[(String, String)],
    now: i64,
) -> Vec<(String /*area*/, String /*content*/)> {
    let mut areas: Vec<(String, String)> = Vec::new();

    for (area, count) in list_dir_roots {
        if *count >= EXPLORATION_LIST_DIR_THRESHOLD {
            if is_sensitive_path(area) {
                continue;
            }
            let content = format_discovery_entry(area, &[("list_dir", *count)], basename_notes, now);
            areas.push((area.clone(), content));
        }
    }

    for (prefix, count) in read_prefixes {
        if *count >= EXPLORATION_READ_THRESHOLD {
            if is_sensitive_path(prefix) {
                continue;
            }
            // Avoid duplicate if already covered as list_dir area prefix.
            if areas
                .iter()
                .any(|(a, _)| a == prefix || a.starts_with(&format!("{prefix}/")))
            {
                continue;
            }
            let content = format_discovery_entry(prefix, &[("read_file", *count)], basename_notes, now);
            areas.push((prefix.clone(), content));
        }
    }

    areas
}

pub fn format_discovery_entry(area: &str, tools: &[(&str, u32)], notes: &[(String, String)], now: i64) -> String {
    let tools_s = tools
        .iter()
        .map(|(name, n)| format!("{name}×{n}"))
        .collect::<Vec<_>>()
        .join(", ");
    let notes_s = if notes.is_empty() {
        "(none)".into()
    } else {
        notes
            .iter()
            .take(MAX_DISCOVERY_NOTES)
            .map(|(name, tool)| format!("{name} ({tool})"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let raw = format!("[discovery] Area: {area}/\nObserved tools: {tools_s}\nNotes: {notes_s}\nObserved at unix={now}");
    truncate_chars(&raw, 500)
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// True if `content` describes the same discovery area (for dedupe).
pub fn discovery_area_matches(content: &str, area: &str) -> bool {
    let needle = format!("Area: {area}");
    content.contains(&needle) || content.contains(&format!("Area: {area}/"))
}

/// Whether path components look like a relative project path (not absolute-only noise).
#[allow(dead_code)]
pub fn is_simple_relative(path: &str) -> bool {
    !Path::new(path)
        .components()
        .any(|c| matches!(c, Component::RootDir | Component::Prefix(_)))
}

/// Display path for journals: redact sensitive; keep short.
pub fn journal_path(path: &str) -> String {
    if is_sensitive_path(path) {
        return "(redacted-sensitive-path)".into();
    }
    let trimmed = path.trim();
    if trimmed.chars().count() > 160 {
        let t: String = trimmed.chars().take(160).collect();
        format!("{t}...")
    } else {
        trimmed.to_string()
    }
}

pub fn format_change_entry(entries: &[(String, String)]) -> String {
    let mut lines = Vec::with_capacity(entries.len() + 2);
    lines.push(format!("[change] tools count={}", entries.len()));
    lines.push("Paths:".into());
    for (path, tool) in entries {
        lines.push(format!("- {} ({tool})", journal_path(path)));
    }
    lines.join("\n")
}

pub fn format_work_entry(prompt_snippet: &str, paths: &[(String, String)], outcome: &str) -> String {
    let goal = truncate_chars(prompt_snippet.trim(), 120);
    let path_list = if paths.is_empty() {
        "(none)".into()
    } else {
        paths
            .iter()
            .map(|(p, _)| journal_path(p))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "[work] {goal}\nPaths: {path_list}\nOutcome: {outcome}\nNote: auto-captured from successful mutations this turn"
    )
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sensitive_env_detected() {
        assert!(is_sensitive_path(".env"));
        assert!(is_sensitive_path("app/.env.local"));
        assert!(is_sensitive_path("secrets/token.json"));
        assert!(!is_sensitive_path("elph/src/memory/hooks.rs"));
    }

    #[test]
    fn extract_edit_path() {
        let paths = paths_from_tool_input("edit_file", &json!({"path": "src/main.rs"}));
        assert_eq!(paths, vec!["src/main.rs"]);
    }

    #[test]
    fn change_entry_redacts_sensitive() {
        let text = format_change_entry(&[(".env".into(), "write_file".into())]);
        assert!(text.contains("redacted-sensitive-path"));
        assert!(!text.contains(".env"));
    }

    #[test]
    fn exploration_threshold_triggers_discovery() {
        let mut scratch = ExplorationScratch::default();
        record_exploration(&mut scratch, "list_dir", &json!({"path": "elph/src/memory"}));
        record_exploration(&mut scratch, "list_dir", &json!({"path": "elph/src/memory"}));
        let entries =
            build_discovery_entries(&scratch.list_dir_roots, &scratch.read_prefixes, &scratch.basename_notes, 42);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].1.contains("[discovery]"));
        assert!(entries[0].1.contains("elph/src/memory"));
        assert!(entries[0].1.contains("list_dir×2"));
        assert!(entries[0].1.contains("unix=42"));
        assert!(!entries[0].1.contains("fn main"));
    }

    #[test]
    fn exploration_below_threshold_no_discovery() {
        let mut scratch = ExplorationScratch::default();
        record_exploration(&mut scratch, "list_dir", &json!({"path": "src"}));
        let entries =
            build_discovery_entries(&scratch.list_dir_roots, &scratch.read_prefixes, &scratch.basename_notes, 1);
        assert!(entries.is_empty());
    }

    #[test]
    fn sensitive_exploration_skipped() {
        let mut scratch = ExplorationScratch::default();
        record_exploration(&mut scratch, "list_dir", &json!({"path": ".env"}));
        record_exploration(&mut scratch, "list_dir", &json!({"path": ".env"}));
        assert!(scratch.list_dir_roots.is_empty());
    }
}
