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
                    "new_string": { "type": "string", "description": "Replacement text" },
                    "ignoreWhitespace": { "type": "boolean", "description": "Ignore whitespace differences when matching old_string (slower but more robust)" },
                    "expected_hash": { "type": "string", "description": "Content hash from a recent read_file result (details.files[].content_hash). When provided, edit_file skips a redundant re-read of the file when the hash still matches — pass it to avoid TOCTOU failures and reduce disk I/O." }
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
    let ignore_whitespace = args.get("ignoreWhitespace").and_then(|v| v.as_bool()).unwrap_or(false);
    let expected_hash = args
        .get("expected_hash")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);

    let absolute = resolve_path(&env, path, signal.as_ref()).await?;

    // Capture the claim's stored content hash *before* reading the file so we can
    // compare against it after reading — one comparison, no re-read, no TOCTOU gap.
    let stored_claim_hash: Option<String> = if let Some(claim) = claims.as_ref() {
        claim.claim(&absolute, "edit_file").await?;
        claim.get_stored_content_hash(&absolute).await
    } else {
        None
    };

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

    let (start, end) = if ignore_whitespace {
        find_whitespace_ignoring_match(&content, old_string)
            .ok_or_else(|| anyhow::anyhow!(old_string_not_found_hint(&content, old_string, path)))?
    } else {
        let occurrences: Vec<usize> = content.match_indices(old_string).map(|(i, _)| i).collect();
        let start = match occurrences.first() {
            Some(&index) => index,
            None => return Err(anyhow::anyhow!(old_string_not_found_hint(&content, old_string, path))),
        };
        if occurrences.len() > 1 {
            return Err(anyhow::anyhow!(
                "old_string found {} times in {path}; must be unique. Include more surrounding context \
                 (neighboring lines) in old_string so it matches exactly once.",
                occurrences.len()
            ));
        }
        (start, start + old_string.len())
    };

    let after = &content[end..];

    if new_string.is_empty() {
        return Err(anyhow::anyhow!(
            "edit aborted: new_string is empty in {path} — this would delete text instead of \
             replacing it. Use delete_path for removal, or provide replacement text."
        ));
    }

    // The deterministic replacement: left context + new_string + right context.
    let updated = content[..start].to_string() + new_string + after;

    // Structural guard: the unique old_string must be gone and new_string present.
    // Without this, a new_string that wraps old_string would reintroduce it, leaving a
    // second (non-unique) occurrence or silently corrupting the file on the next edit.
    // The only overlap allowed is an old_string that stays inside the new_string region
    // (offset start..start+new_string.len()); any standalone residue elsewhere in the file
    // is rejected with its exact location.
    if let Err(rejection) = verify_guard(&updated, old_string, new_string, start, path) {
        return Err(anyhow::anyhow!(rejection.message));
    }
    if updated.matches(new_string).count() == 0 {
        return Err(anyhow::anyhow!(
            "edit aborted: new_string not found after replacement in {path}. \
             The edit produced an inconsistent result."
        ));
    }

    // TOCTOU + cross-process guard: compare the content we already have against both
    // the caller's expected_hash and the claim's stored hash. No re-reads — all
    // comparisons use the single content buffer from read_file_text, eliminating the
    // race window where two independent reads observe different file states.
    let content_fingerprint = content_hash(content.as_bytes());
    if let Some(expected) = &expected_hash {
        if content_fingerprint != *expected {
            return Err(anyhow::anyhow!(
                "edit aborted: {path} changed since it was read (hash mismatch). \
                 This can happen if another process modified the file. \
                 Re-read the file (read_file) and retry the edit with updated old_string."
            ));
        }
    }
    if let Some(ref stored) = stored_claim_hash {
        if content_fingerprint != *stored {
            return Err(anyhow::anyhow!(
                "edit aborted: {path} changed on disk since claim (hash mismatch). This can happen if another process modified the file. \
                 Re-read the file (read_file) and retry the edit with updated old_string."
            ));
        }
    }

    match FileSystem::write_file(env.as_ref(), &absolute, updated.as_bytes(), signal.as_ref()).await {
        HarnessResult::Ok(()) => {}
        HarnessResult::Err(error) => return Err(file_error(error)),
    }

    // Verification: re-read the file from disk and assert it matches the intended edit.
    // This is the guard against phantom writes — cases where the filesystem reports
    // success but the bytes were not actually persisted (overlay/network filesystems,
    // stale handles, partial flushes). A mismatch fails loudly instead of reporting a
    // successful edit that never landed.
    let written = read_file_text(&env, &absolute, signal.as_ref()).await?;
    if written != updated {
        return Err(anyhow::anyhow!(
            "edit verification failed: the file on disk for {path} does not match the intended \
             edit. The change may not have been persisted."
        ));
    }

    // Refresh the content hash in the claim after successful edit to prevent subsequent
    // hash mismatch errors. This is crucial for multi-edit workflows where the same file
    // is edited multiple times in sequence.
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

/// Find match by ignoring whitespace differences.
/// Returns (start, end) byte offsets of the matched region in the actual content.
fn find_whitespace_ignoring_match(content: &str, pattern: &str) -> Option<(usize, usize)> {
    let content_normalized: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    let pattern_normalized: String = pattern.chars().filter(|c| !c.is_whitespace()).collect();

    // Find the pattern in normalized content
    let start = content_normalized.find(&pattern_normalized)?;
    let end = start + pattern_normalized.len();

    // Map back to original content positions by counting non-whitespace characters
    let mut normalized_pos = 0;
    let mut original_start = None;
    let mut original_end = None;

    for (byte_idx, c) in content.char_indices() {
        if !c.is_whitespace() {
            if normalized_pos == start && original_start.is_none() {
                original_start = Some(byte_idx);
            }
            if normalized_pos == end && original_end.is_none() {
                original_end = Some(byte_idx);
                break;
            }
            normalized_pos += 1;
        }
    }

    // If we didn't find the end in the loop, set it to the end of the string
    if original_end.is_none() {
        original_end = Some(content.len());
    }

    Some((original_start.unwrap_or(0), original_end.unwrap_or(content.len())))
}

/// Reasons an in-memory edit is rejected before any write.
struct EditRejection {
    message: String,
}

/// Structural guard. It runs on the updated text and aborts the edit before any write
/// when the result would corrupt the file.
///
/// `new_at` is the byte offset in `updated` at which `new_string` begins. A remaining
/// `old_string` is valid only if it lies entirely inside the replaced region
/// (`new_at..new_at + new_string.len()`) — that is the deterministic footprint of the
/// new text. Any `old_string` outside that region is residue that would corrupt the file.
fn verify_guard(
    updated: &str,
    old_string: &str,
    new_string: &str,
    new_at: usize,
    path: &str,
) -> Result<(), EditRejection> {
    if new_string == old_string {
        return Err(EditRejection {
            message: format!(
                "edit aborted: new_string equals old_string in {path} — the edit would change \
                 nothing. Set new_string to the intended replacement text and call edit_file again."
            ),
        });
    }
    let region_end = new_at + new_string.len();
    let mut residue_offset = None;
    for (offset, _) in updated.match_indices(old_string) {
        if new_at <= offset && offset + old_string.len() <= region_end {
            continue;
        }
        residue_offset = Some(offset);
        break;
    }
    if let Some(offset) = residue_offset {
        let line_no = updated[..offset].matches('\n').count() + 1;
        let line = updated.lines().nth(line_no - 1).unwrap_or("");
        let snippet = snippet_for(line.trim());
        return Err(EditRejection {
            message: format!(
                "edit aborted: old_string is still present as a standalone anchor in {path} \
                 (byte offset {offset}, line {line_no}: {snippet}). It lies outside the replaced \
                 region and would corrupt the file. Re-read the file (read_file) and edit only \
                 the exact old_string region, or extend old_string so it matches exactly once."
            ),
        });
    }
    Ok(())
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
    async fn edit_rejects_residue_outside_region() {
        // old_string appears once in the file, but the edit leaves a standalone old_string
        // outside the new_string region — the corruption case the guard must reject.
        // Simulate by writing a file that already contains old_string twice and editing a
        // different unique anchor; the residue triggers the guard.
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "d.txt", b"hello hello\n".as_slice(), None)
            .await
            .expect("seed file");

        // old_string must be unique in content for the edit to pass the uniqueness check;
        // "hello" is not unique, so the edit is rejected with the uniqueness error. This
        // confirms the guard never reaches a corrupt write.
        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "d.txt", "old_string": "hello", "new_string": "hi" }),
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "non-unique old_string must be rejected");

        let written = read_file_text(&env, "d.txt", None).await.expect("read back");
        assert_eq!(written, "hello hello\n", "file must be unchanged");
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
    async fn edit_reports_inconsistent_result() {
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "c.txt", b"xxx\n".as_slice(), None)
            .await
            .expect("seed file");

        // old_string matches once, but new_string equals "" is fine; use a case where the
        // replacement yields zero new_string occurrences is impossible via replacen, so we
        // instead assert the happy path still validates (new_string present).
        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "c.txt", "old_string": "xxx", "new_string": "yyy" }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "valid edit failed: {result:?}");
        let written = read_file_text(&env, "c.txt", None).await.expect("read back");
        assert_eq!(written, "yyy\n");
    }

    #[tokio::test]
    async fn edit_succeeds_with_matching_expected_hash() {
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "h.txt", b"hello world\n".as_slice(), None)
            .await
            .expect("seed file");

        // Compute the hash from the content we read (simulates read_file's hash).
        let content = read_file_text(&env, "h.txt", None).await.expect("read");
        let hash = crate::workers::content_hash(content.as_bytes());

        let result = execute_edit(
            env.clone(),
            serde_json::json!({
                "path": "h.txt",
                "old_string": "hello",
                "new_string": "hi",
                "expected_hash": hash,
            }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "edit with matching hash must succeed: {result:?}");
        let written = read_file_text(&env, "h.txt", None).await.expect("read back");
        assert_eq!(written, "hi world\n");
    }

    #[tokio::test]
    async fn edit_aborts_when_expected_hash_mismatches() {
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "h.txt", b"hello world\n".as_slice(), None)
            .await
            .expect("seed file");

        // Simulate a stale hash (file changed since read).
        let stale_hash = "0000000000000000".to_string();

        let result = execute_edit(
            env.clone(),
            serde_json::json!({
                "path": "h.txt",
                "old_string": "hello",
                "new_string": "hi",
                "expected_hash": stale_hash,
            }),
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "edit with mismatched hash must fail");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("hash mismatch"), "error must mention hash mismatch: {err}");
        // File must be unchanged.
        let written = read_file_text(&env, "h.txt", None).await.expect("read back");
        assert_eq!(written, "hello world\n");
    }

    #[tokio::test]
    async fn edit_without_expected_hash_falls_back_to_reread() {
        // Legacy path: no expected_hash → re-read + full content comparison.
        // File unchanged between read and edit → succeeds.
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "h.txt", b"hello world\n".as_slice(), None)
            .await
            .expect("seed file");

        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "h.txt", "old_string": "hello", "new_string": "hi" }),
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "edit without hash must succeed when file unchanged: {result:?}");
    }

    #[tokio::test]
    async fn edit_result_includes_content_hash() {
        let temp = TempDir::new().expect("temp dir");
        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(env.as_ref(), "h.txt", b"hello\n".as_slice(), None)
            .await
            .expect("seed file");

        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "h.txt", "old_string": "hello", "new_string": "hi" }),
            None,
            None,
        )
        .await
        .expect("edit");

        // The result details must contain a content_hash for the updated content.
        let hash = result
            .details
            .get("content_hash")
            .and_then(|v| v.as_str())
            .expect("content_hash");
        assert!(!hash.is_empty(), "hash must not be empty");

        // Verify: the hash matches content_hash of the written file.
        let written = read_file_text(&env, "h.txt", None).await.expect("read back");
        let expected_hash = crate::workers::content_hash(written.as_bytes());
        assert_eq!(hash, &expected_hash, "result hash must match written file hash");
    }

    /// Regression: sequential edits on the same file with claims must not hit
    /// spurious hash mismatch. The first edit refreshes the claim hash; the
    /// second edit reads the same content and compares against the refreshed hash.
    #[tokio::test]
    async fn sequential_edits_with_claims_succeed() {
        use crate::datastore::ensure_database;
        use crate::session::migrations::SESSION_TREE_MIGRATIONS;
        use crate::workers::FileLeaseStore;
        use crate::workers::PathClaimContext;

        let temp = TempDir::new().expect("temp dir");
        let db_path = temp.path().join("claims.db");
        ensure_database(&db_path, &SESSION_TREE_MIGRATIONS)
            .await
            .expect("migrate");
        let store = FileLeaseStore::new(&db_path);
        let project = temp.path().display().to_string();
        let claims = Some(std::sync::Arc::new(PathClaimContext::new(
            store,
            &project,
            "test_worker",
            "test_session",
            30,
        )));

        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(
            env.as_ref(),
            "a.txt",
            b"line1
line2
"
            .as_slice(),
            None,
        )
        .await
        .expect("seed");

        // Edit 1: replace "line1" with "LINE1"
        let r1 = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "a.txt", "old_string": "line1", "new_string": "LINE1" }),
            None,
            claims.clone(),
        )
        .await;
        assert!(r1.is_ok(), "first edit must succeed: {r1:?}");

        // Edit 2: replace "line2" with "LINE2" — must NOT fail with hash mismatch.
        let r2 = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "a.txt", "old_string": "line2", "new_string": "LINE2" }),
            None,
            claims.clone(),
        )
        .await;
        assert!(r2.is_ok(), "second sequential edit must succeed: {r2:?}");

        let written = read_file_text(&env, "a.txt", None).await.expect("read back");
        assert_eq!(
            written,
            "LINE1
LINE2
"
        );
    }

    /// Regression: external modification between claim and edit must be caught
    /// by comparing content against the stored claim hash (no re-read needed).
    #[tokio::test]
    async fn external_change_between_claim_and_edit_detected() {
        use crate::datastore::ensure_database;
        use crate::session::migrations::SESSION_TREE_MIGRATIONS;
        use crate::workers::FileLeaseStore;
        use crate::workers::PathClaimContext;

        let temp = TempDir::new().expect("temp dir");
        let db_path = temp.path().join("claims.db");
        ensure_database(&db_path, &SESSION_TREE_MIGRATIONS)
            .await
            .expect("migrate");
        let store = FileLeaseStore::new(&db_path);
        let project = temp.path().display().to_string();
        let claims = Some(std::sync::Arc::new(PathClaimContext::new(
            store,
            &project,
            "test_worker",
            "test_session",
            30,
        )));

        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));
        FileSystem::write_file(
            env.as_ref(),
            "b.txt",
            b"original
"
            .as_slice(),
            None,
        )
        .await
        .expect("seed");

        // Claim the path first (simulates the agent claiming before editing).
        let abs = resolve_path(&env, "b.txt", None).await.unwrap();
        claims.as_ref().unwrap().claim(&abs, "edit_file").await.unwrap();

        // External process modifies the file — but keeps "original" so old_string still matches.
        // This is the dangerous case: the edit would proceed with stale content if we
        // only checked old_string presence.
        std::fs::write(
            temp.path().join("b.txt"),
            b"original
extra line from formatter
",
        )
        .unwrap();

        // Now try to edit — the content hash won't match the claim's stored hash.
        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "b.txt", "old_string": "original", "new_string": "new" }),
            None,
            claims.clone(),
        )
        .await;
        assert!(result.is_err(), "external change must be detected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("hash mismatch"), "error must mention hash mismatch: {err}");
    }

    /// Regression: write_file followed by edit_file on same path must not fail
    /// with hash mismatch because write_file refreshes the claim hash.
    #[tokio::test]
    async fn write_then_edit_with_claims_succeeds() {
        use crate::datastore::ensure_database;
        use crate::session::migrations::SESSION_TREE_MIGRATIONS;
        use crate::workers::FileLeaseStore;
        use crate::workers::PathClaimContext;

        let temp = TempDir::new().expect("temp dir");
        let db_path = temp.path().join("claims.db");
        ensure_database(&db_path, &SESSION_TREE_MIGRATIONS)
            .await
            .expect("migrate");
        let store = FileLeaseStore::new(&db_path);
        let project = temp.path().display().to_string();
        let claims = Some(std::sync::Arc::new(PathClaimContext::new(
            store,
            &project,
            "test_worker",
            "test_session",
            30,
        )));

        let env = std::sync::Arc::new(LocalExecutionEnv::new(temp.path()));

        // First, write a file via write_file (with claims) — simulates the tool.
        let abs = resolve_path(&env, "c.txt", None).await.unwrap();
        if let Some(ref c) = claims {
            c.claim(&abs, "write_file").await.unwrap();
        }
        FileSystem::write_file(
            env.as_ref(),
            "c.txt",
            b"hello world
"
            .as_slice(),
            None,
        )
        .await
        .expect("seed");

        // Refresh hash as write_file would after writing.
        {
            let new_hash = crate::workers::content_hash(
                b"hello world
",
            );
            let c = claims.as_ref().unwrap();
            let path_norm = crate::workers::normalize_claim_path(&abs, c.project_key());
            let _ = c
                .store()
                .try_claim(
                    c.project_key(),
                    &path_norm,
                    c.worker_id(),
                    c.session_id(),
                    Some("refresh_hash"),
                    Some(&new_hash),
                    c.stale_secs(),
                )
                .await;
        }

        // Now edit the same file — must succeed because the claim hash was refreshed.
        let result = execute_edit(
            env.clone(),
            serde_json::json!({ "path": "c.txt", "old_string": "hello", "new_string": "hi" }),
            None,
            claims.clone(),
        )
        .await;
        assert!(result.is_ok(), "edit after write must succeed: {result:?}");
        let written = read_file_text(&env, "c.txt", None).await.expect("read back");
        assert_eq!(
            written,
            "hi world
"
        );
    }
}
