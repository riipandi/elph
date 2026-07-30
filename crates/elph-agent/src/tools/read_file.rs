//! Read tool — elph coding-agent tools.
//!
//! Reads file contents with support for:
//! - Single file with optional offset/limit
//! - Batch reading of multiple files in one call
//! - Multiple specific ranges across files

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
                "Read file contents from the project. Supports single files with offset/limit, \
                 batch reading of multiple files (paths), and multiple specific ranges (ranges). \
                 Each file's output is truncated to {DEFAULT_MAX_LINES} lines or {}/KB.",
                DEFAULT_MAX_BYTES / 1024
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to a single file to read (relative or absolute)"
                    },
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Multiple file paths to read in one call. Mutually exclusive with 'path'."
                    },
                    "offset": {
                        "type": "number",
                        "description": "Line number to start reading from (1-indexed). Applies to all files when used with 'paths'."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of lines to read. Applies to all files when used with 'paths'."
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
                        "description": "Multiple specific file ranges to read. Each entry specifies a path and optional offset/limit."
                    }
                },
                "oneOf": [
                    { "required": ["path"] },
                    { "required": ["paths"] },
                    { "required": ["ranges"] }
                ]
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
    let batch_mode = requests.len() > 1;

    for (i, request) in requests.iter().enumerate() {
        let absolute = resolve_path(&env, &request.path, signal.as_ref()).await?;
        if is_probably_image(&absolute) {
            all_outputs.push(format!("[{}] Read image file (content omitted)", request.path));
            continue;
        }

        let content = read_file_text(&env, &absolute, signal.as_ref()).await?;
        let start_line = request.offset.map(|v| v.saturating_sub(1)).unwrap_or(0);
        let selected =
            match crate::agent::harness::utils::truncate::select_line_range(&content, start_line, request.limit) {
                Ok(selected) => selected,
                Err(total_lines) => {
                    return Err(anyhow::anyhow!(
                        "Offset {} is beyond end of file ({} lines total) in {}",
                        request.offset.unwrap_or(1),
                        total_lines,
                        request.path,
                    ));
                }
            };

        let truncation = truncate_head(&selected, TruncationOptions::default());
        let mut output = truncation.content;
        if truncation.first_line_exceeds_limit {
            output = format!(
                "[Line {} exceeds {} limit in {}. Use shell_exec to read a portion of the file.]",
                start_line + 1,
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

        if batch_mode {
            let header = if let Some(offset) = request.offset {
                if let Some(limit) = request.limit {
                    format!("--- {} (lines {}-{}) ---", request.path, offset, offset + limit - 1)
                } else {
                    format!("--- {} (from line {}) ---", request.path, offset)
                }
            } else {
                format!("--- {} ---", request.path)
            };

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
        }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
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
        assert!(text.contains("--- a.txt ---"));
        assert!(text.contains("--- b.txt ---"));
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
}
