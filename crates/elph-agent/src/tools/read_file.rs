//! Read tool — elph coding-agent tools.
//!
//! Reads file contents with support for:
//! - Single file with optional offset/limit (line-range streaming — does not load
//!   the whole file when offset/limit is set)
//! - Batch reading of multiple files in one call
//! - Multiple specific ranges across files
//! - Large file handling with automatic truncation and size limits

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;

use elph_ai::Tool;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::utils::truncate::TruncationOptions;
use crate::agent::harness::utils::truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES};
use crate::agent::harness::utils::truncate::{format_size, truncate_head};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::common::{check_aborted, is_probably_image, read_file_text, resolve_path};
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};
use crate::workers::content_hash;

/// Maximum file size we'll attempt to read (100MB)
const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;

/// A single file read request (path + optional range).
#[derive(Debug, Clone)]
struct ReadRequest {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

pub fn create_read_file_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let env_for_tool = env.clone();
    simple_tool(
        Tool {
            name: "read_file".into(),
            constrained_sampling: None,
            description: format!(
                "Read file contents. Prefer offset/limit (or ranges) after grep hits — do not load whole large files. \
Batch with paths[] for multiple known files in one call. Windowed reads include line numbers and a (start-end of total) header. \
Truncates to {DEFAULT_MAX_LINES} lines or {}/KB per file.",
                DEFAULT_MAX_BYTES / 1024
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to a single file to read (relative or absolute). Use one of: path, paths, or ranges."
                    },
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Multiple file paths to read in one call. Prefer this over sequential read_file calls."
                    },
                    "offset": {
                        "type": "number",
                        "description": "1-indexed start line. Prefer with limit for large files (streams without full load)."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of lines to read from offset."
                    },
                    "ranges": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "offset": { "type": "number" },
                                "limit": { "type": "number" }
                            },
                            "required": ["path"]
                        },
                        "description": "Multiple specific file ranges (path + offset/limit) in one call."
                    }
                }
                // No root oneOf: xAI rejects oneOf branches that only list `required`
                // without `"type":"object"`. Mutual exclusivity is enforced at runtime.
            }),
        },
        "read_file",
        move |_, args| {
            let env = env_for_tool.clone();
            Box::pin(async move { execute_read(env, args, None).await })
        },
    )
}

async fn execute_read(
    env: Arc<LocalExecutionEnv>,
    args: Value,
    signal: Option<CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;

    // --- Parse read requests ---
    let requests = parse_read_requests(&args)?;

    let mut all_outputs: Vec<String> = Vec::new();
    let mut all_hashes: Vec<Value> = Vec::new();
    let batch_mode = requests.len() > 1;

    for (i, request) in requests.iter().enumerate() {
        let absolute = resolve_path(&env, &request.path, signal.as_ref()).await?;
        if is_probably_image(&absolute) {
            all_outputs.push(format!("[{}] Read image file (content omitted)", request.path));
            continue;
        }

        check_aborted(signal.as_ref())?;
        
        // Check file size before reading to handle large files
        let file_size = std::fs::metadata(&absolute)
            .map(|m| m.len())
            .unwrap_or(0);
        
        if file_size > MAX_FILE_SIZE {
            all_outputs.push(format!(
                "[{}] File too large to read ({} bytes > {} bytes). Use offset/limit to read specific ranges.",
                request.path,
                format_size(file_size as usize),
                format_size(MAX_FILE_SIZE as usize)
            ));
            continue;
        }

        let ranged = request.offset.is_some() || request.limit.is_some();
        let (body, meta) = if ranged {
            // Stream only the requested window — O(offset+limit) I/O, not full file.
            read_line_window(&absolute, request.offset, request.limit)?
        } else {
            let content = read_file_text(&env, &absolute, signal.as_ref()).await?;
            let total = content.lines().count();
            (
                content,
                ReadWindowMeta {
                    start_line: 1,
                    end_line: total,
                    total_lines: total,
                    truncated_by_eof: false,
                },
            )
        };

        // Content hash from bytes already in memory — free, no extra disk I/O.
        // Enables edit_file to skip a TOCTOU re-read when the hash still matches.
        let hash = content_hash(body.as_bytes());
        all_hashes.push(json!({
            "path": request.path,
            "content_hash": hash,
        }));

        let truncation = truncate_head(&body, TruncationOptions::default());
        let mut output = truncation.content;
        if truncation.first_line_exceeds_limit {
            output = format!(
                "[Line {} exceeds {} limit in {}. Use a smaller offset/limit window.]",
                meta.start_line,
                format_size(DEFAULT_MAX_BYTES),
                request.path,
            );
        } else if truncation.truncated {
            output.push_str(&format!(
                "\n\n[Truncated: showing first {} lines / {}]",
                truncation.output_lines,
                format_size(truncation.output_bytes)
            ));
        }

        // Line numbers when reading a window (matches Grok-style range reads).
        if ranged && !output.starts_with('[') {
            output = number_lines(&output, meta.start_line);
        }

        let header = if ranged {
            format!(
                "--- {} ({}-{} of {}) ---",
                request.path, meta.start_line, meta.end_line, meta.total_lines
            )
        } else {
            format!("--- {} ---", request.path)
        };

        if batch_mode || ranged {
            if i > 0 {
                all_outputs.push(String::new());
            }
            all_outputs.push(header);
            all_outputs.push(output);
        } else {
            all_outputs.push(output);
        }
    }

    let result_text = all_outputs.join("\n");

    Ok(AgentToolResult {
        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(
            result_text,
        ))],
        details: json!({
            "file_count": requests.len(),
            "files": all_hashes,
        }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
}

struct ReadWindowMeta {
    start_line: usize,
    end_line: usize,
    total_lines: usize,
    #[allow(dead_code)]
    truncated_by_eof: bool,
}

/// Read `[offset, offset+limit)` lines (1-indexed offset) without loading the full file.
fn read_line_window(
    absolute: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> anyhow::Result<(String, ReadWindowMeta)> {
    let start = offset.unwrap_or(1).max(1);
    let max_lines = limit.unwrap_or(DEFAULT_MAX_LINES).max(1);

    let file =
        std::fs::File::open(Path::new(absolute)).map_err(|e| anyhow::anyhow!("Failed to open {absolute}: {e}"))?;
    let reader = BufReader::new(file);

    let mut selected = String::new();
    let mut total = 0usize;
    let mut end_line = start.saturating_sub(1);
    let mut taken = 0usize;

    for line in reader.lines() {
        let line = line.map_err(|e| anyhow::anyhow!("Failed to read {absolute}: {e}"))?;
        total += 1;
        if total < start {
            continue;
        }
        if taken >= max_lines {
            // Keep counting remaining lines for accurate "of N" totals (cheap for text).
            // Stop after a modest scan beyond the window to avoid multi-second reads on huge files.
            // Cap residual count at start+max_lines+10_000.
            if total >= start + max_lines + 10_000 {
                // Approximate remainder: report at least known total.
                break;
            }
            continue;
        }
        selected.push_str(&line);
        selected.push('\n');
        taken += 1;
        end_line = total;
    }

    if total < start {
        return Err(anyhow::anyhow!(
            "Offset {start} is beyond end of file ({total} lines total) in {absolute}"
        ));
    }

    // If we stopped early on residual scan, note total as lower bound via max.
    if end_line < start {
        end_line = start;
    }

    Ok((
        selected,
        ReadWindowMeta {
            start_line: start,
            end_line,
            total_lines: total.max(end_line),
            truncated_by_eof: taken < max_lines,
        },
    ))
}

fn number_lines(content: &str, start_line: usize) -> String {
    let mut out = String::with_capacity(content.len() + content.lines().count() * 6);
    for (i, line) in content.lines().enumerate() {
        let n = start_line + i;
        out.push_str(&format!("{n:>6}|{line}\n"));
    }
    // Preserve trailing newline absence for empty.
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Parse read requests from tool arguments.
fn parse_read_requests(args: &Value) -> anyhow::Result<Vec<ReadRequest>> {
    // Ranges take priority: multiple specific paths with individual offsets
    if let Some(ranges) = args.get("ranges").and_then(|v| v.as_array()) {
        if ranges.is_empty() {
            return Err(anyhow::anyhow!("'ranges' must contain at least one entry"));
        }
        let mut requests = Vec::with_capacity(ranges.len());
        for range in ranges {
            let path = range
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Each range entry must have a 'path' field"))?
                .to_string();
            let offset = range.get("offset").and_then(|v| v.as_u64()).map(|v| v as usize);
            let limit = range.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);
            requests.push(ReadRequest { path, offset, limit });
        }
        return Ok(requests);
    }

    // Batch paths: multiple files with shared offset/limit
    if let Some(paths) = args.get("paths").and_then(|v| v.as_array()) {
        if paths.is_empty() {
            return Err(anyhow::anyhow!("'paths' must contain at least one path"));
        }
        let offset = args.get("offset").and_then(|v| v.as_u64()).map(|v| v as usize);
        let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);
        let requests: Vec<ReadRequest> = paths
            .iter()
            .filter_map(|v| v.as_str())
            .map(|p| ReadRequest {
                path: p.to_string(),
                offset,
                limit,
            })
            .collect();
        if requests.is_empty() {
            return Err(anyhow::anyhow!("'paths' must contain valid file paths"));
        }
        return Ok(requests);
    }

    // Single file (legacy mode)
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: 'path', 'paths', or 'ranges'"))?
        .to_string();
    let offset = args.get("offset").and_then(|v| v.as_u64()).map(|v| v as usize);
    let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);

    Ok(vec![ReadRequest { path, offset, limit }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    fn setup_env() -> (Arc<LocalExecutionEnv>, TempDir) {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("a.txt"), "line1\nline2\nline3\nline4\nline5\n").expect("write");
        fs::write(dir.path().join("b.txt"), "alpha\nbeta\ngamma\n").expect("write");
        (Arc::new(LocalExecutionEnv::new(dir.path().to_path_buf())), dir)
    }

    #[tokio::test]
    async fn read_single_file() {
        let (env, _dir) = setup_env();
        let args = json!({"path": "a.txt"});
        let result = execute_read(env, args, None).await.expect("read failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("line1"));
        assert!(text.contains("line5"));
    }

    #[tokio::test]
    async fn read_with_offset() {
        let (env, _dir) = setup_env();
        let args = json!({"path": "a.txt", "offset": 3});
        let result = execute_read(env, args, None).await.expect("read failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("line3"));
        assert!(!text.contains("line1"));
        // Windowed reads include a range header and line numbers.
        assert!(text.contains("of "));
        assert!(text.contains("3|") || text.contains("line3"));
    }

    #[tokio::test]
    async fn read_batch_paths() {
        let (env, _dir) = setup_env();
        let args = json!({"paths": ["a.txt", "b.txt"], "limit": 2});
        let result = execute_read(env, args, None).await.expect("read failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("a.txt"));
        assert!(text.contains("b.txt"));
        assert!(text.contains("line1"));
        assert!(text.contains("alpha"));
    }

    #[tokio::test]
    async fn read_ranges() {
        let (env, _dir) = setup_env();
        let args = json!({
            "ranges": [
                {"path": "a.txt", "offset": 1, "limit": 2},
                {"path": "b.txt", "offset": 3, "limit": 1}
            ]
        });
        let result = execute_read(env, args, None).await.expect("read failed");
        let text = result
            .content
            .first()
            .and_then(|c| match c {
                crate::types::ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        assert!(text.contains("a.txt"));
        assert!(text.contains("b.txt"));
        assert!(text.contains("line1"));
        assert!(text.contains("line2"));
        assert!(text.contains("gamma"));
    }

    #[tokio::test]
    async fn read_errors_on_missing_arg() {
        let (env, _dir) = setup_env();
        let args = json!({});
        let result = execute_read(env, args, None).await;
        assert!(result.is_err());
    }

    #[test]
    fn parse_single_path() {
        let args = json!({"path": "main.rs", "offset": 5, "limit": 10});
        let requests = parse_read_requests(&args).expect("parse");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "main.rs");
        assert_eq!(requests[0].offset, Some(5));
        assert_eq!(requests[0].limit, Some(10));
    }

    #[test]
    fn parse_batch_paths() {
        let args = json!({"paths": ["a.rs", "b.rs"], "limit": 20});
        let requests = parse_read_requests(&args).expect("parse");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].path, "a.rs");
        assert_eq!(requests[0].limit, Some(20));
        assert_eq!(requests[1].path, "b.rs");
    }

    #[test]
    fn parse_ranges() {
        let args = json!({
            "ranges": [
                {"path": "a.rs", "offset": 10, "limit": 5},
                {"path": "b.rs"}
            ]
        });
        let requests = parse_read_requests(&args).expect("parse");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].path, "a.rs");
        assert_eq!(requests[0].offset, Some(10));
        assert_eq!(requests[0].limit, Some(5));
        assert_eq!(requests[1].path, "b.rs");
        assert_eq!(requests[1].offset, None);
        assert_eq!(requests[1].limit, None);
    }

    #[test]
    fn parse_ranges_empty_errors() {
        let args = json!({"ranges": []});
        assert!(parse_read_requests(&args).is_err());
    }

    #[tokio::test]
    async fn read_result_includes_content_hash() {
        let (env, dir) = setup_env();
        let args = json!({"path": "a.txt"});
        let result = execute_read(env, args, None).await.expect("read failed");

        let files = result
            .details
            .get("files")
            .and_then(|v| v.as_array())
            .expect("files array");
        assert_eq!(files.len(), 1, "one file");
        let hash = files[0]
            .get("content_hash")
            .and_then(|v| v.as_str())
            .expect("content_hash");
        assert!(!hash.is_empty(), "hash must not be empty");

        // Verify: the hash matches content_hash of the file content.
        let content = std::fs::read_to_string(dir.path().join("a.txt")).expect("read file");
        let expected = crate::workers::content_hash(content.as_bytes());
        assert_eq!(hash, &expected, "read hash must match file content hash");
    }

    #[tokio::test]
    async fn read_batch_includes_per_file_hashes() {
        let (env, _dir) = setup_env();
        let args = json!({"paths": ["a.txt", "b.txt"]});
        let result = execute_read(env, args, None).await.expect("read failed");

        let files = result
            .details
            .get("files")
            .and_then(|v| v.as_array())
            .expect("files array");
        assert_eq!(files.len(), 2, "two files");
        for entry in files {
            let hash = entry
                .get("content_hash")
                .and_then(|v| v.as_str())
                .expect("content_hash per file");
            assert!(!hash.is_empty(), "hash must not be empty");
        }
    }
}
