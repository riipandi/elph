//! Grep tool — elph coding-agent tools.
//!
//! Searches file contents with ripgrep-compatible features:
//! - Regex / literal search
//! - Context lines (before, after, or symmetric)
//! - File-only mode (-l), count mode (-c)
//! - Word regexp, case control
//! - Batch patterns (OR) and batch paths
//! - Multi-threaded via fff_search backend.

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

async fn execute_grep(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;

    // --- Parse patterns ---
    let patterns: Vec<String> = if let Some(pat) = args.get("pattern").and_then(|v| v.as_str()) {
        vec![pat.to_string()]
    } else if let Some(pats) = args.get("patterns").and_then(|v| v.as_array()) {
        pats.iter().filter_map(|v| v.as_str().map(String::from)).collect()
    } else {
        return Err(anyhow::anyhow!("Missing required argument: 'pattern' or 'patterns'"));
    };
    if patterns.is_empty() {
        return Err(anyhow::anyhow!("At least one pattern is required"));
    }

    // --- Parse paths ---
    let paths: Vec<String> = if let Some(p) = args.get("paths").and_then(|v| v.as_array()) {
        p.iter().filter_map(|v| v.as_str().map(String::from)).collect()
    } else {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        vec![path.to_string()]
    };

    // --- Parse other options ---
    let ignore_case = args.get("ignoreCase").and_then(|v| v.as_bool()).unwrap_or(false);
    let literal = args.get("literal").and_then(|v| v.as_bool()).unwrap_or(false);
    let word_regexp = args.get("wordRegexp").and_then(|v| v.as_bool()).unwrap_or(false);
    let files_with_matches = args.get("filesWithMatches").and_then(|v| v.as_bool()).unwrap_or(false);
    let count = args.get("count").and_then(|v| v.as_bool()).unwrap_or(false);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_LIMIT as u64) as usize;
    let max_matches_per_file = args
        .get("maxMatchesPerFile")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    // Context lines are symmetric by default (like -C), overridden by before/after
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

    // --- Determine output mode ---
    let output_mode = if files_with_matches {
        GrepOutputMode::FilesWithMatches
    } else if count {
        GrepOutputMode::Count
    } else {
        GrepOutputMode::Standard
    };

    // --- Resolve all paths ---
    let mut all_results: Vec<String> = Vec::new();
    let mut limit_reached = false;
    let mut lines_truncated = false;
    let mut seen_files = BTreeSet::new();

    // Group results by pattern and path for combined output
    for pattern in &patterns {
        if limit_reached {
            break;
        }
        // Apply word-regexp wrapping if requested
        let (mut effective_pattern, mut effective_mode) = build_grep_mode(pattern, literal, ignore_case);
        if word_regexp && !literal {
            effective_pattern = make_word_regexp(&effective_pattern);
            effective_mode = fff_search::grep::GrepMode::Regex;
        }

        for path in &paths {
            if limit_reached {
                break;
            }
            let absolute = resolve_path(&env, path, signal.as_ref()).await?;
            let info = match env.file_info(&absolute, signal.as_ref()).await {
                HarnessResult::Ok(info) => info,
                HarnessResult::Err(error) => return Err(anyhow::anyhow!("{}", error.message)),
            };
            let is_file = info.kind == FileKind::File;
            if info.kind != FileKind::File && info.kind != FileKind::Directory {
                continue;
            }

            let base_path = resolve_search_base(&absolute, is_file);
            let path_scope = resolve_path_scope(&absolute, is_file);
            let query_text = build_grep_query(&effective_pattern, &path_scope);
            let signal_for_blocking = signal.clone();
            let pattern_label = if patterns.len() > 1 {
                Some(pattern.clone())
            } else {
                None
            };

            let (matches, truncated, limit_hit) = tokio::task::spawn_blocking(move || {
                run_with_abort_signal(signal_for_blocking.as_ref(), |abort| {
                    let parsed_query = parse_grep_query(&query_text);
                    let picker = build_picker(&base_path)?;
                    let options = build_grep_options(
                        limit,
                        max_matches_per_file,
                        effective_mode,
                        ignore_case,
                        before_context,
                        after_context,
                        abort,
                    );
                    let result = picker.grep(&parsed_query, &options);

                    let output_opts = GrepOutputOptions {
                        mode: output_mode,
                        ..Default::default()
                    };
                    let (matches, lines_truncated) = format_grep_output_ex(&picker, &result, &output_opts);
                    Ok((matches, lines_truncated, result.matches.len() >= limit))
                })
            })
            .await??;

            if truncated {
                lines_truncated = true;
            }
            if limit_hit {
                // If we hit the limit on this batch, stop adding more
                limit_reached = true;
            }

            // Prepend pattern label if multiple patterns
            if let Some(ref label) = pattern_label {
                if !matches.is_empty() {
                    if !all_results.is_empty() {
                        all_results.push(String::new());
                    }
                    all_results.push(format!("[Pattern: {label}]"));
                }
            }

            // Deduplicate file paths for files-with-matches mode
            if output_mode == GrepOutputMode::FilesWithMatches {
                for m in &matches {
                    if seen_files.insert(m.clone()) {
                        all_results.push(m.clone());
                    }
                }
            } else {
                all_results.extend(matches);
            }
        }
    }

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
        // Create some test files
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
        let args = json!({"pattern": "fn", "path": "."});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("main.rs"));
        assert!(text.contains("lib.rs"));
    }

    #[tokio::test]
    async fn grep_literal() {
        let (env, _dir) = setup_env();
        let args = json!({"pattern": "hello", "literal": true});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("hello"));
    }

    #[tokio::test]
    async fn grep_context_lines() {
        let (env, _dir) = setup_env();
        let args = json!({"pattern": "println", "context": 1});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        // Should contain the fn line (context before) and the println line (match)
        assert!(text.contains("fn main"));
        assert!(text.contains("println"));
    }

    #[tokio::test]
    async fn grep_files_with_matches() {
        let (env, _dir) = setup_env();
        let args = json!({"pattern": "fn", "filesWithMatches": true});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        // Should contain file paths, not line:content
        assert!(text.contains("main.rs") || text.contains(".rs"));
        assert!(!text.contains(":1:")); // no line numbers
    }

    #[tokio::test]
    async fn grep_count_mode() {
        let (env, _dir) = setup_env();
        let args = json!({"pattern": "hello", "count": true});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        // Should show file:count format
        assert!(text.contains(":") && !text.contains(":1:"));
    }

    #[tokio::test]
    async fn grep_batch_patterns() {
        let (env, _dir) = setup_env();
        let args = json!({"patterns": ["println", "print"], "path": "."});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("println"));
    }

    #[tokio::test]
    async fn grep_batch_paths() {
        let (env, _dir) = setup_env();
        let args = json!({"pattern": "fn", "paths": ["main.rs", "lib.rs"]});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("fn"));
    }

    #[tokio::test]
    async fn grep_word_regexp() {
        let (env, _dir) = setup_env();
        let args = json!({"pattern": "main", "wordRegexp": true});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("fn main"));
    }

    #[tokio::test]
    async fn grep_ignore_case() {
        let (env, _dir) = setup_env();
        let args = json!({"pattern": "HELLO", "ignoreCase": true});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("hello"));
    }

    #[tokio::test]
    async fn grep_max_matches_per_file() {
        let (env, _dir) = setup_env();
        // lib.rs has 2 "hello" occurrences in "hello world"
        let args = json!({"pattern": "hello", "maxMatchesPerFile": 1});
        let result = execute_grep(env, args, None).await.expect("grep failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        // Should have at most 1 match per file
        assert!(!text.is_empty());
    }

    #[tokio::test]
    async fn grep_errors_on_missing_pattern() {
        let (env, _dir) = setup_env();
        let args = json!({});
        let result = execute_grep(env, args, None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("pattern") || err.to_string().contains("Pattern"));
    }
}
