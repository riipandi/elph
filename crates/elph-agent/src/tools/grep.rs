//! Grep tool — elph coding-agent tools.
//!
//! Searches file contents with ripgrep-compatible features:
//! - Regex / literal search
//! - Context lines (before, after, or symmetric)
//! - File-only mode (-l), count mode (-c)
//! - Word regexp, case control
//! - Batch patterns (OR) and batch paths
//! - One picker build per unique directory — efficient for N-pattern × M-path.

use std::collections::BTreeSet;
use std::sync::Arc;

use elph_ai::Tool;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::{FileKind, FileSystem, Result as HarnessResult};
use crate::agent::harness::utils::truncate::DEFAULT_MAX_BYTES;
use crate::agent::harness::utils::truncate::TruncationOptions;
use crate::agent::harness::utils::truncate::truncate_head;
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, resolve_path};
use crate::tools::fff_picker::{
    GrepOutputMode, GrepOutputOptions, build_grep_mode, build_grep_options, build_grep_query, build_picker,
    format_grep_output_ex, make_word_regexp, parse_grep_query, resolve_path_scope, resolve_search_base,
    run_with_abort_signal,
};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

const DEFAULT_LIMIT: usize = 200;

pub fn create_grep_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "grep".into(),
            constrained_sampling: None,

            description: "Search file contents with regex or literal patterns across the project. \
                         Supports context lines (-C), file-only listing (-l), match counts (-c), \
                         whole-word matching (-w), case-insensitive search, batch patterns, and batch paths."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Single regex or literal pattern to search for"
                    },
                    "patterns": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Multiple patterns combined with OR logic. Mutually exclusive with 'pattern'."
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory or file to search (default: current directory)"
                    },
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Multiple directories/files to search. Results from all paths are combined."
                    },
                    "context": {
                        "type": "number",
                        "description": "Lines of context before and after each match (like grep -C)"
                    },
                    "beforeContext": {
                        "type": "number",
                        "description": "Lines of context before each match (like grep -B)"
                    },
                    "afterContext": {
                        "type": "number",
                        "description": "Lines of context after each match (like grep -A)"
                    },
                    "filesWithMatches": {
                        "type": "boolean",
                        "description": "Only list filenames with matches (like grep -l)"
                    },
                    "count": {
                        "type": "boolean",
                        "description": "Show match count per file (like grep -c)"
                    },
                    "wordRegexp": {
                        "type": "boolean",
                        "description": "Match whole words only (like grep -w)"
                    },
                    "ignoreCase": {
                        "type": "boolean",
                        "description": "Case-insensitive search (like grep -i)"
                    },
                    "literal": {
                        "type": "boolean",
                        "description": "Treat pattern as literal text, not regex"
                    },
                    "maxMatchesPerFile": {
                        "type": "number",
                        "description": "Maximum matches per file before stopping (like grep --max-count)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum total matches to return (default: 200)"
                    }
                },
                "anyOf": [
                    { "required": ["pattern"] },
                    { "required": ["patterns"] }
                ]
            }),
        },
        "grep",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_grep(env, args, None).await })
        },
    )
}

// ── Internal types ──────────────────────────────────────────────

#[derive(Clone)]
struct PreparedPattern {
    raw: String,
    query: String,
    mode: fff_search::grep::GrepMode,
}

struct SearchTarget {
    /// Base directory for the picker.
    base_path: String,
    /// Path scope for the query (empty for directory, filename for file).
    path_scope: String,
}

// ── Main entry ─────────────────────────────────────────────────

async fn execute_grep(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;

    // ── Parse patterns ──────────────────────────────────────────
    let raw_patterns: Vec<String> = if let Some(pat) = args.get("pattern").and_then(|v| v.as_str()) {
        vec![pat.to_string()]
    } else if let Some(pats) = args.get("patterns").and_then(|v| v.as_array()) {
        pats.iter().filter_map(|v| v.as_str().map(String::from)).collect()
    } else {
        return Err(anyhow::anyhow!("Missing required argument: 'pattern' or 'patterns'"));
    };
    if raw_patterns.is_empty() {
        return Err(anyhow::anyhow!("At least one pattern is required"));
    }

    let ignore_case = args.get("ignoreCase").and_then(|v| v.as_bool()).unwrap_or(false);
    let literal = args.get("literal").and_then(|v| v.as_bool()).unwrap_or(false);
    let word_regexp = args.get("wordRegexp").and_then(|v| v.as_bool()).unwrap_or(false);

    // Build patterns once — they are Clone, reused for every target.
    let patterns: Vec<PreparedPattern> = raw_patterns
        .iter()
        .map(|raw| {
            let (mut effective_pattern, mut effective_mode) = build_grep_mode(raw, literal, ignore_case);
            if word_regexp && !literal {
                effective_pattern = make_word_regexp(&effective_pattern);
                effective_mode = fff_search::grep::GrepMode::Regex;
            }
            PreparedPattern {
                raw: raw.clone(),
                query: effective_pattern,
                mode: effective_mode,
            }
        })
        .collect();

    let multi_pattern = patterns.len() > 1;

    // ── Parse options (copied scalars, no ownership issues) ────
    let files_with_matches = args.get("filesWithMatches").and_then(|v| v.as_bool()).unwrap_or(false);
    let count = args.get("count").and_then(|v| v.as_bool()).unwrap_or(false);
    let output_mode = if files_with_matches {
        GrepOutputMode::FilesWithMatches
    } else if count {
        GrepOutputMode::Count
    } else {
        GrepOutputMode::Standard
    };
    let limit: usize = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_LIMIT as u64) as usize;
    let max_matches_per_file = args
        .get("maxMatchesPerFile")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let context = args
        .get("context")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(0);
    let before_context = args
        .get("beforeContext")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(context);
    let after_context = args
        .get("afterContext")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(context);

    // ── Parse & deduplicate paths ──────────────────────────────
    let raw_paths: Vec<String> = if let Some(p) = args.get("paths").and_then(|v| v.as_array()) {
        p.iter().filter_map(|v| v.as_str().map(String::from)).collect()
    } else {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        vec![path.to_string()]
    };

    let mut targets: Vec<SearchTarget> = Vec::new();
    let mut seen_bases: BTreeSet<String> = BTreeSet::new();
    for raw in &raw_paths {
        let absolute = resolve_path(&env, raw, signal.as_ref()).await?;
        let info = match env.file_info(&absolute, signal.as_ref()).await {
            HarnessResult::Ok(info) => info,
            HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
        };
        if info.kind != FileKind::File && info.kind != FileKind::Directory {
            continue;
        }
        let is_file = info.kind == FileKind::File;
        let base_path = resolve_search_base(&absolute, is_file);
        if !seen_bases.insert(base_path.clone()) {
            continue;
        }
        let path_scope = resolve_path_scope(&absolute, is_file);
        targets.push(SearchTarget { base_path, path_scope });
    }

    let multi_target = targets.len() > 1;

    // ── Execute ────────────────────────────────────────────────
    let mut all_results: Vec<String> = Vec::new();
    let mut limit_reached = false;
    let mut lines_truncated = false;

    for target in &targets {
        if limit_reached {
            break;
        }

        let t_base = target.base_path.clone();
        let t_scope = target.path_scope.clone();
        let sig = signal.clone();
        // Clone patterns so each thread owns its copy (not moved on first iter).
        let patterns_for_thread = patterns.clone();

        // One picker build per target; all patterns run through it.
        let (target_results, truncated) = tokio::task::spawn_blocking(move || {
            run_with_abort_signal(sig.as_ref(), |abort| {
                let picker = build_picker(&t_base)?;
                let mut out: Vec<String> = Vec::new();
                let mut truncated = false;

                for pattern in patterns_for_thread.iter() {
                    // Borrow when no scope; allocate when scope needed.
                    let query_cow = if t_scope.is_empty() {
                        std::borrow::Cow::Borrowed(&pattern.query)
                    } else {
                        std::borrow::Cow::Owned(build_grep_query(&pattern.query, &t_scope))
                    };
                    let parsed = parse_grep_query(&query_cow);

                    let opts = build_grep_options(
                        limit,
                        max_matches_per_file,
                        pattern.mode,
                        false,
                        before_context,
                        after_context,
                        abort.clone(),
                    );
                    let result = picker.grep(&parsed, &opts);

                    let fmt_opts = GrepOutputOptions {
                        mode: output_mode,
                        ..Default::default()
                    };
                    let (matches, lt) = format_grep_output_ex(&picker, &result, &fmt_opts);
                    if lt {
                        truncated = true;
                    }

                    if multi_pattern && !matches.is_empty() {
                        if !out.is_empty() {
                            out.push(String::new());
                        }
                        out.push(format!("[Pattern: {}]", pattern.raw));
                    }
                    out.extend(matches);

                    if result.matches.len() >= limit {
                        break;
                    }
                }

                Ok((out, truncated))
            })
        })
        .await??;

        if truncated {
            lines_truncated = true;
        }

        if multi_target && !target_results.is_empty() {
            if !all_results.is_empty() {
                all_results.push(String::new());
            }
            all_results.push(format!("[Path: {}]", target.base_path));
        }

        let prev = all_results.len();
        all_results.extend(target_results);
        if all_results.len() - prev >= limit {
            limit_reached = true;
        }
    }

    // Deduplicate file paths across targets for files-with-matches
    if output_mode == GrepOutputMode::FilesWithMatches {
        let mut seen = BTreeSet::new();
        all_results.retain(|line| line.starts_with('[') || seen.insert(line.clone()));
    }

    // ── Truncate and format output ─────────────────────────────
    let output = all_results.join("\n");
    let truncation = truncate_head(
        &output,
        TruncationOptions {
            max_bytes: Some(DEFAULT_MAX_BYTES),
            max_lines: None,
        },
    );
    let mut text = truncation.content;
    if limit_reached {
        text.push_str(&format!("\n\n[{limit} matches limit]"));
    }
    if truncation.truncated {
        text.push_str("\n\n[output truncated]");
    } else if lines_truncated {
        text.push_str("\n\n[some lines truncated for length]");
    }

    Ok(AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "matchLimitReached": limit_reached,
            "linesTruncated": lines_truncated,
            "truncated": truncation.truncated
        }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    fn setup_env() -> (Arc<LocalExecutionEnv>, TempDir) {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("main.rs"), "fn main() {\n    println!(\"hello\");\n}\n").expect("write");
        fs::write(
            dir.path().join("lib.rs"),
            "pub fn greet() {\n    println!(\"hello world\");\n}\n",
        )
        .expect("write");
        fs::write(dir.path().join("hello.py"), "def hello():\n    print('hello')\n").expect("write");
        (Arc::new(LocalExecutionEnv::new(dir.path().to_path_buf())), dir)
    }

    #[tokio::test]
    async fn grep_basic_regex() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"pattern": "fn", "path": "."}), None)
            .await
            .expect("grep");
        let text = tool_text(&result);
        assert!(text.contains("main.rs") && text.contains("lib.rs"));
    }

    #[tokio::test]
    async fn grep_literal() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"pattern": "hello", "literal": true}), None)
            .await
            .expect("grep");
        assert!(tool_text(&result).contains("hello"));
    }

    #[tokio::test]
    async fn grep_context_lines() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"pattern": "println", "context": 1}), None)
            .await
            .expect("grep");
        let text = tool_text(&result);
        assert!(text.contains("fn main"));
        assert!(text.contains("println"));
    }

    #[tokio::test]
    async fn grep_files_with_matches() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"pattern": "fn", "filesWithMatches": true}), None)
            .await
            .expect("grep");
        let text = tool_text(&result);
        assert!((text.contains("main.rs") || text.contains(".rs")) && !text.contains(":1:"));
    }

    #[tokio::test]
    async fn grep_count_mode() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"pattern": "hello", "count": true}), None)
            .await
            .expect("grep");
        let text = tool_text(&result);
        assert!(text.contains(":") && !text.contains(":1:"));
    }

    #[tokio::test]
    async fn grep_batch_patterns() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"patterns": ["println", "print"], "path": "."}), None)
            .await
            .expect("grep");
        let text = tool_text(&result);
        assert!(text.contains("println") && text.contains("[Pattern:"));
    }

    #[tokio::test]
    async fn grep_batch_paths() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"pattern": "fn", "paths": ["main.rs", "lib.rs"]}), None)
            .await
            .expect("grep");
        assert!(tool_text(&result).contains("fn"));
    }

    #[tokio::test]
    async fn grep_word_regexp() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"pattern": "main", "wordRegexp": true}), None)
            .await
            .expect("grep");
        assert!(tool_text(&result).contains("fn main"));
    }

    #[tokio::test]
    async fn grep_ignore_case() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"pattern": "HELLO", "ignoreCase": true}), None)
            .await
            .expect("grep");
        assert!(tool_text(&result).contains("hello"));
    }

    #[tokio::test]
    async fn grep_max_matches_per_file() {
        let (env, _dir) = setup_env();
        let result = execute_grep(env, json!({"pattern": "hello", "maxMatchesPerFile": 1}), None)
            .await
            .expect("grep");
        assert!(!tool_text(&result).is_empty());
    }

    #[tokio::test]
    async fn grep_errors_on_missing_pattern() {
        let (env, _dir) = setup_env();
        assert!(execute_grep(env, json!({}), None).await.is_err());
    }

    fn tool_text(result: &AgentToolResult) -> &str {
        match &result.content[0] {
            crate::types::ToolResultContent::Text(t) => t.text.as_str(),
            _ => "",
        }
    }
}
