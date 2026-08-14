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
use crate::workers::{SharedPathClaim, content_hash};

/// Maximum file size we'll attempt to edit (100MB)
const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;

pub fn create_edit_file_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    create_edit_file_tool_with_claims(env, None)
}

pub fn create_edit_file_tool_with_claims(env: Arc<LocalExecutionEnv>, claims: SharedPathClaim) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "edit_file".into(),
            constrained_sampling: None,
            description: "Edits files by replacing exact text. Use when you have specific old_string from a recent read_file. \
                 For fuzzy matching or structural changes, use write_file instead. Copy old_string verbatim from the file — \
                 whitespace matters. If file changed on disk, re-read and retry. If old_string appears multiple times, \
                 include more context to make it unique."
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
            let claims = claims.clone();
            Box::pin(async move { execute_edit(env, args, None, claims).await })
        },
    )
}

async fn execute_edit(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
    claims: SharedPathClaim,
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

    if let Some(claim) = claims.as_ref() {
        claim.claim(&absolute, "edit_file").await?;
    }

    // Check file size before editing to handle large files
    let file_size = std::fs::metadata(&absolute).map(|m| m.len()).unwrap_or(0);

    if file_size > MAX_FILE_SIZE {
        return Err(anyhow::anyhow!(
            "File too large to edit ({} bytes > {} bytes). Use write_file for complete rewrites or split the file.",
            file_size,
            MAX_FILE_SIZE
        ));
    }

    let content = read_file_text(&env, &absolute, signal.as_ref()).await?;

    let (start, end) = match find_match(&content, old_string) {
        MatchResult::Unique(start, end) => (start, end),
        MatchResult::Multiple(count) => {
            return Err(anyhow::anyhow!(
                "old_string found {count} times in {path}; must be unique. Include more surrounding context \
                 (neighboring lines) in old_string so it matches exactly once.",
            ));
        }
        MatchResult::NotFound => {
            return Err(anyhow::anyhow!(old_string_not_found_hint(&content, old_string, path)));
        }
    };

    if new_string == &content[start..end] {
        return Err(anyhow::anyhow!(
            "edit aborted: replacement text is identical to matched text in {path} — the edit would change nothing."
        ));
    }

    let updated = content[..start].to_string() + new_string + &content[end..];

    match FileSystem::write_file(env.as_ref(), &absolute, updated.as_bytes(), signal.as_ref()).await {
        HarnessResult::Ok(()) => {}
        HarnessResult::Err(error) => return Err(file_error(error)),
    }

    // Refresh content hash in the claim after successful edit
    if let Some(claim) = claims.as_ref() {
        let new_hash = content_hash(updated.as_bytes());
        let path_norm = crate::workers::normalize_claim_path(&absolute, claim.project_key());
        let _ = claim
            .store()
            .try_claim(
                claim.project_key(),
                &path_norm,
                claim.worker_id(),
                claim.session_id(),
                Some("refresh_hash"),
                Some(&new_hash),
                claim.stale_secs(),
            )
            .await;
    }

    Ok(AgentToolResult {
        content: vec![ToolResultContent::Text(elph_ai::TextContent::new(format!(
            "Edited {path}"
        )))],
        details: json!({
            "old_content": content,
            "new_content": updated,
            "file_path": absolute,
            "content_hash": content_hash(updated.as_bytes()),
        }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
}

enum MatchResult {
    Unique(usize, usize),
    Multiple(usize),
    NotFound,
}

/// Robust multi-strategy finder:
/// 1. Exact string match
/// 2. Line-ending normalized match (CRLF vs LF)
/// 3. Line-by-line trimmed block match
fn find_match(content: &str, pattern: &str) -> MatchResult {
    // 1. Exact match
    let exact_matches: Vec<usize> = content.match_indices(pattern).map(|(i, _)| i).collect();
    if exact_matches.len() == 1 {
        let start = exact_matches[0];
        return MatchResult::Unique(start, start + pattern.len());
    }
    if exact_matches.len() > 1 {
        return MatchResult::Multiple(exact_matches.len());
    }

    // 2. Line-ending normalized match (\r\n vs \n)
    if pattern.contains('\r') || content.contains('\r') {
        let pattern_lf = pattern.replace("\r\n", "\n");
        let content_lf = content.replace("\r\n", "\n");
        let lf_matches: Vec<usize> = content_lf.match_indices(&pattern_lf).map(|(i, _)| i).collect();
        if lf_matches.len() == 1 {
            // Map byte index from LF string back to original content
            if let Some((start, end)) = map_lf_range_to_orig(content, lf_matches[0], pattern_lf.len()) {
                return MatchResult::Unique(start, end);
            }
        }
        if lf_matches.len() > 1 {
            return MatchResult::Multiple(lf_matches.len());
        }
    }

    // 3. Line-trimmed block match (matches multi-line code where line indentation may slightly differ)
    let pattern_lines: Vec<&str> = pattern.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if pattern_lines.len() >= 2 {
        let content_lines: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .map(|(i, l)| (i, l.trim()))
            .filter(|(_, l)| !l.is_empty())
            .collect();

        let mut matched_starts = Vec::new();
        for window in content_lines.windows(pattern_lines.len()) {
            let matches = window
                .iter()
                .zip(&pattern_lines)
                .all(|((_, c_line), p_line)| c_line == p_line);
            if matches {
                matched_starts.push((window[0].0, window.last().unwrap().0));
            }
        }

        if matched_starts.len() == 1 {
            let (start_line_idx, end_line_idx) = matched_starts[0];
            let mut current_line = 0;
            let mut byte_start = None;
            let mut byte_end = None;

            for (idx, line) in content.split_inclusive('\n').enumerate() {
                if idx == start_line_idx {
                    byte_start = Some(content[..content.len() - content.split_inclusive('\n').skip(idx).map(|s| s.len()).sum::<usize>()].len());
                }
                if idx == end_line_idx {
                    let before = content.split_inclusive('\n').take(idx + 1).map(|s| s.len()).sum::<usize>();
                    // strip trailing newline if pattern did not end with newline
                    let line_len = if !pattern.ends_with('\n') && line.ends_with('\n') {
                        if line.ends_with("\r\n") { line.len() - 2 } else { line.len() - 1 }
                    } else {
                        line.len()
                    };
                    let start_of_this_line = before - line.len();
                    byte_end = Some(start_of_this_line + line_len);
                    break;
                }
                current_line += 1;
            }
            let _ = current_line;

            // Recalculate cleanly
            let lines: Vec<&str> = content.split_inclusive('\n').collect();
            if start_line_idx < lines.len() && end_line_idx < lines.len() {
                let start_offset: usize = lines[..start_line_idx].iter().map(|l| l.len()).sum();
                let end_offset: usize = lines[..=end_line_idx].iter().map(|l| l.len()).sum();
                let mut matched_slice = &content[start_offset..end_offset];
                if !pattern.ends_with('\n') {
                    matched_slice = matched_slice.trim_end_matches(['\r', '\n']);
                }
                return MatchResult::Unique(start_offset, start_offset + matched_slice.len());
            }
        } else if matched_starts.len() > 1 {
            return MatchResult::Multiple(matched_starts.len());
        }
    }

    MatchResult::NotFound
}

fn map_lf_range_to_orig(orig: &str, lf_start: usize, lf_len: usize) -> Option<(usize, usize)> {
    let mut orig_idx = 0;
    let mut lf_idx = 0;
    let mut orig_start = None;
    let mut orig_end = None;

    let orig_bytes = orig.as_bytes();
    while orig_idx < orig_bytes.len() {
        if lf_idx == lf_start && orig_start.is_none() {
            orig_start = Some(orig_idx);
        }
        if lf_idx == lf_start + lf_len && orig_end.is_none() {
            orig_end = Some(orig_idx);
            break;
        }

        if orig_bytes[orig_idx] == b'\r' && orig_idx + 1 < orig_bytes.len() && orig_bytes[orig_idx + 1] == b'\n' {
            orig_idx += 2;
            lf_idx += 1;
        } else {
            orig_idx += 1;
            lf_idx += 1;
        }
    }

    if lf_idx == lf_start + lf_len && orig_end.is_none() {
        orig_end = Some(orig_idx);
    }

    match (orig_start, orig_end) {
        (Some(s), Some(e)) => Some((s, e)),
        _ => None,
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
    use crate::agent::harness::types::FileSystem;
    use crate::runtime::local_env::LocalExecutionEnv;
    use tempfile::TempDir;

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

    #[tokio::test]
    async fn edit_persists_and_verifies_on_disk() {
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "a.txt", b"hello world\n".as_slice(), None)
            .await
            .expect("seed file");

        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "a.txt", "old_string": "hello", "new_string": "hi" }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "edit failed: {result:?}");

        let written = read_file_text(&env, "a.txt", None).await.expect("read back");
        assert_eq!(written, "hi world\n", "edit did not persist on disk");
    }

    #[tokio::test]
    async fn edit_rejects_non_unique_old_string() {
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "b.txt", b"abc\nabc\n".as_slice(), None)
            .await
            .expect("seed file");

        // old_string appears twice -> must be rejected before any write.
        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "b.txt", "old_string": "a", "new_string": "x" }),
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "non-unique old_string must be rejected");

        let written = read_file_text(&env, "b.txt", None).await.expect("read back");
        assert_eq!(written, "abc\nabc\n", "file must be unchanged after a rejected edit");
    }

    #[tokio::test]
    async fn edit_allows_new_containing_old_within_region() {
        // new_string "axa" contains old_string "a", but the old occurrence stays inside
        // the new_string region; no standalone old_string remains outside it. This is a
        // legitimate deliberate edit (the old guard rejected it with a false positive).
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "c.txt", b"abc\n".as_slice(), None)
            .await
            .expect("seed file");

        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "c.txt", "old_string": "a", "new_string": "axa" }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "containing-within-region must succeed: {result:?}");

        let written = read_file_text(&env, "c.txt", None).await.expect("read back");
        assert_eq!(written, "axabc\n", "file must contain the intended replacement");
    }

    #[tokio::test]
    async fn edit_allows_adjacent_append_overlap() {
        // A right-side append that starts with old_string (e.g. opening a tag next to
        // its closing tag, or wrapping a line) is valid: old_string no longer appears
        // standalone, but `new_string` legitimately begins with it. The old guard
        // rejected this with a false positive.
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "d.txt", b"<div></div>\n".as_slice(), None)
            .await
            .expect("seed file");

        let result = execute_edit(
            env.clone(),
            serde_json::json!({
                "path": "d.txt",
                "old_string": "</div>",
                "new_string": "</div></div>",
            }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "adjacent append must succeed: {result:?}");

        let written = read_file_text(&env, "d.txt", None).await.expect("read back");
        assert_eq!(written, "<div></div></div>\n", "file must contain the appended tag");
    }

    #[tokio::test]
    async fn edit_allows_duplicate_old_at_original_seam() {
        // old_string appears uniquely in the file, and new_string duplicates it at the
        // seam followed by more text. The result has two occurrences, but only one is
        // the replaced region; the other is the untouched original.
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "e.txt", b"begin mid end\n".as_slice(), None)
            .await
            .expect("seed file");

        let result = execute_edit(
            env.clone(),
            serde_json::json!({
                "path": "e.txt",
                "old_string": "mid",
                "new_string": "mid-mid",
            }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "duplicate at seam must succeed: {result:?}");

        let written = read_file_text(&env, "e.txt", None).await.expect("read back");
        assert_eq!(written, "begin mid-mid end\n", "new_string must replace the first mid");
    }

    #[tokio::test]
    async fn edge_allows_new_without_old_residue() {
        // new_string "yay" does not contain old_string "xxx"; the replaced region has no
        // old_string at all, and no residue remains. Valid.
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "f.txt", b"xxx\n".as_slice(), None)
            .await
            .expect("seed file");

        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "f.txt", "old_string": "xxx", "new_string": "yay" }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "replacement without residue must succeed: {result:?}");

        let written = read_file_text(&env, "f.txt", None).await.expect("read back");
        assert_eq!(written, "yay\n", "file must contain the replacement");
    }

    #[tokio::test]
    async fn edge_allows_new_prefix_of_old_with_extra_text() {
        // new_string "ol" is a prefix of old_string "old_string". The full old_string is
        // gone, and the replacement is the exact bytes of new_string. Valid.
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "g.txt", b"old_string\n".as_slice(), None)
            .await
            .expect("seed file");

        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "g.txt", "old_string": "old_string", "new_string": "ol" }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "prefix replacement must succeed: {result:?}");

        let written = read_file_text(&env, "g.txt", None).await.expect("read back");
        assert_eq!(written, "ol\n", "file must contain the replacement");
    }

    #[tokio::test]
    async fn edge_rejects_new_equals_old_noop() {
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "h.txt", b"same\n".as_slice(), None)
            .await
            .expect("seed file");

        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "h.txt", "old_string": "same", "new_string": "same" }),
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "no-op edit must be rejected");
    }

    #[tokio::test]
    async fn edit_handles_crlf_line_endings() {
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "crlf.txt", b"line1\r\nline2\r\nline3\r\n".as_slice(), None)
            .await
            .expect("seed file");

        // Search with LF but file has CRLF
        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "crlf.txt", "old_string": "line1\nline2\n", "new_string": "line1_updated\r\nline2_updated\r\n" }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "crlf matching must succeed: {result:?}");

        let written = read_file_text(&env, "crlf.txt", None).await.expect("read back");
        assert_eq!(written, "line1_updated\r\nline2_updated\r\nline3\r\n");
    }

    #[tokio::test]
    async fn edit_handles_multiline_trimmed_match() {
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        let original = "fn test() {\n    let a = 1;\n    let b = 2;\n    println!(\"{}\", a + b);\n}\n";
        FileSystem::write_file(env.as_ref(), "trim.rs", original.as_bytes(), None)
            .await
            .expect("seed file");

        // Slightly different indentation in search query
        let old = "let a = 1;\n        let b = 2;\n        println!(\"{}\", a + b);";
        let new = "let sum = 3;\n    println!(\"{}\", sum);";

        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "trim.rs", "old_string": old, "new_string": new }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "trimmed multiline matching must succeed: {result:?}");

        let written = read_file_text(&env, "trim.rs", None).await.expect("read back");
        assert!(written.contains("let sum = 3;"));
        assert!(!written.contains("let a = 1;"));
    }
}
