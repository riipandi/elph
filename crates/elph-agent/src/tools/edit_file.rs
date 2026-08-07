//! Edit tool — elph coding-agent tools.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::{FileSystem, Result as HarnessResult};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, file_error, read_file_text, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult, ToolResultContent};

pub fn create_edit_file_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "edit_file".into(),
            constrained_sampling: None,

            description: "Edits files by replacing specific text with new content. Provide all three arguments: \
                 path (file to edit), old_string (exact existing text — must match the file exactly, \
                 including whitespace, and appear exactly once), new_string (replacement text). Copy \
                 old_string verbatim from a recent read_file result; if the file may have been \
                 reformatted (e.g. cargo fmt), re-read it first so old_string still matches."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file to edit" },
                    "old_string": { "type": "string", "description": "Text to replace (must match exactly once)" },
                    "new_string": { "type": "string", "description": "Replacement text" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        "edit_file",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_edit(env, args, None).await })
        },
    )
}

async fn execute_edit(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(missing_required("path"))?;
    let old_string = args
        .get("old_string")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(missing_required("old_string"))?;
    let new_string = args
        .get("new_string")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(missing_required("new_string"))?;

    let absolute = resolve_path(&env, path, signal.as_ref()).await?;
    let content = read_file_text(&env, &absolute, signal.as_ref()).await?;
    let count = content.matches(old_string).count();
    if count == 0 {
        return Err(anyhow::anyhow!(old_string_not_found_hint(&content, old_string, path)));
    }
    if count > 1 {
        return Err(anyhow::anyhow!(
            "old_string found {count} times in {path}; must be unique. Include more surrounding context (neighboring lines) in old_string so it matches exactly once."
        ));
    }
    let updated = content.replacen(old_string, new_string, 1);
    match FileSystem::write_file(env.as_ref(), &absolute, updated.as_bytes(), signal.as_ref()).await {
        HarnessResult::Ok(()) => Ok(AgentToolResult {
            content: vec![ToolResultContent::Text(elph_ai::TextContent::new(format!(
                "Edited {path}"
            )))],
            details: json!({
                "old_content": content,
                "new_content": updated,
                "file_path": absolute,
            }),
            added_tool_names: None,
            terminate: None,
            usage: None,
        }),
        HarnessResult::Err(error) => Err(file_error(error)),
    }
}

/// Clear "missing argument" error that tells the model exactly what edit_file needs.
fn missing_required(name: &str) -> impl FnOnce() -> anyhow::Error + '_ {
    move || {
        anyhow::anyhow!(
            "Missing required argument: {name}. edit_file requires path, old_string, and new_string \
             — re-read the file first (read_file), then call edit_file again with all three."
        )
    }
}

/// Build a useful error when `old_string` does not appear in the file.
///
/// Failure modes handled:
/// 1. Whitespace drift (cargo fmt / reindentation) — detects a whitespace-insensitive match.
/// 2. Misquoted text — points at the file line that most resembles the anchor (first
///    non-empty line) of `old_string` so the model can re-read the exact bytes.
fn old_string_not_found_hint(content: &str, old_string: &str, path: &str) -> String {
    let base = format!("old_string not found in {path}");

    // 1) Whitespace-insensitive match — the text is there, but formatting drifted.
    let old_words: Vec<&str> = old_string.split_whitespace().collect();
    if !old_words.is_empty() && old_words.len() <= 64 {
        let content_words: Vec<&str> = content.split_whitespace().collect();
        if content_words.windows(old_words.len()).any(|w| w == old_words) {
            return format!(
                "{base}. The text matches after ignoring whitespace — cargo fmt or reindentation \
                 likely reformatted it. Re-read the file (read_file) and copy the exact text."
            );
        }
    }

    // 2) Closest single line to the anchor line of old_string.
    let anchor = old_string
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(old_string);
    if !anchor.is_empty()
        && let Some((line_no, snippet)) = nearest_line(content, anchor)
    {
        return format!(
            "{base}. Closest match is line {line_no}: {snippet} — whitespace or formatting differs, \
             re-read the file (read_file) and copy the exact text."
        );
    }

    format!("{base}. Re-read the file with read_file and copy the exact text.")
}

/// Find the content line most similar to `anchor` (word-level overlap), limited to
/// lines whose word count is plausibly close. Returns 1-based line number + snippet.
fn nearest_line(content: &str, anchor: &str) -> Option<(usize, String)> {
    const MAX_LINES_SCANNED: usize = 5000;
    const MIN_SCORE: f64 = 0.45;

    let anchor_words: Vec<&str> = anchor.split_whitespace().collect();
    if anchor_words.is_empty() {
        return None;
    }
    let max_words = anchor_words.len().saturating_mul(3).max(anchor_words.len() + 4);

    let mut best: Option<(f64, usize, String)> = None;
    for (idx, line) in content.lines().enumerate().take(MAX_LINES_SCANNED) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_words: Vec<&str> = trimmed.split_whitespace().collect();
        if line_words.len() > max_words {
            continue;
        }
        let score = word_overlap(&anchor_words, &line_words);
        if score > MIN_SCORE && best.as_ref().is_none_or(|(bs, _, _)| score > *bs) {
            best = Some((score, idx + 1, snippet_for(trimmed)));
        }
    }
    best.map(|(_, line_no, snippet)| (line_no, snippet))
}

/// Fraction of anchor words also present in the line, weighted by line length:
/// `2 * common / (|anchor| + |line|)`. Case-insensitive.
fn word_overlap(anchor: &[&str], line: &[&str]) -> f64 {
    if anchor.is_empty() {
        return 0.0;
    }
    let mut common = 0usize;
    for word in anchor {
        if line.iter().any(|w| w.eq_ignore_ascii_case(word)) {
            common += 1;
        }
    }
    (2.0 * common as f64) / (anchor.len() + line.len()) as f64
}

/// Truncate a matched line for embedding in an error message.
fn snippet_for(line: &str) -> String {
    const MAX: usize = 80;
    if line.chars().count() <= MAX {
        line.to_string()
    } else {
        format!("{}…", line.chars().take(MAX).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_detects_whitespace_drift() {
        let content = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
        let old = "fn main() {\n        let x = 1;\n";
        let hint = old_string_not_found_hint(content, old, "src/main.rs");
        assert!(hint.contains("whitespace"), "{hint}");
        assert!(hint.contains("src/main.rs"), "{hint}");
    }

    #[test]
    fn hint_points_at_nearest_line() {
        let content = "fn alpha() {}\nfn beta() {\n    do_thing();\n}\n";
        let old = "fn betta() {\n    do_thing();\n}";
        let hint = old_string_not_found_hint(content, old, "src/lib.rs");
        assert!(hint.contains("line 2"), "{hint}");
        assert!(hint.contains("fn beta()"), "{hint}");
    }

    #[test]
    fn hint_falls_back_to_generic() {
        let content = "completely unrelated\ncontent here\n";
        let old = "struct Widget { id: u64 }";
        let hint = old_string_not_found_hint(content, old, "src/lib.rs");
        assert!(hint.contains("Re-read the file"), "{hint}");
    }

    #[test]
    fn overlap_is_case_insensitive() {
        let anchor = ["fn", "Foo"];
        let line = ["fn", "foo"];
        assert!(word_overlap(&anchor, &line) > 0.9);
        assert!(word_overlap(&anchor, &["bar"]) < 0.5);
    }

    #[test]
    fn snippet_truncates_long_lines() {
        let long = "x".repeat(200);
        let s = snippet_for(&long);
        assert!(s.chars().count() <= 81, "len={}", s.chars().count());
        assert!(s.ends_with('…'));
    }

    #[test]
    fn missing_required_mentions_all_args() {
        let err = missing_required("path")().to_string();
        assert!(err.contains("path"), "{err}");
        assert!(err.contains("old_string"), "{err}");
        assert!(err.contains("new_string"), "{err}");
    }
}
