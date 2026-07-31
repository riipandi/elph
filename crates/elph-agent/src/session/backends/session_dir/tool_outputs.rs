//! Append-only tool execution output persistence.
//!
//! Every tool call + result is written to `tool_outputs.jsonl` in the session
//! directory. This provides a durable, append-only log that survives session
//! resume and can be browsed from the TUI.
//!
//! Each line is a JSON object with the tool call ID, name, arguments, output,
//! error flag, and timestamp.
//!
//! ## Format
//!
//! ```jsonl
//! {"call_id":"t-01","tool_name":"read_file","args":{"path":"src/main.rs"},"output":"fn main() {}","is_error":false,"timestamp":"2026-07-30T23:00:00Z"}
//! {"call_id":"t-02","tool_name":"edit_file","args":{"path":"src/main.rs","old_string":"fn main()","new_string":"fn run()"},"output":"Edited src/main.rs","is_error":false,"timestamp":"2026-07-30T23:00:01Z"}
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use super::layout::TOOL_OUTPUTS_FILE;
use crate::session::types::{SessionError, SessionErrorCode};

/// One entry in the tool outputs log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutputEntry {
    /// Tool call identifier (stable across turns).
    pub call_id: String,
    /// Tool name (e.g. `read_file`, `edit_file`, `shell_exec`).
    pub tool_name: String,
    /// Tool arguments as JSON.
    pub args: Value,
    /// Tool output text (truncated to [`MAX_OUTPUT_LENGTH`] chars).
    pub output: String,
    /// Whether the tool execution failed.
    pub is_error: bool,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

/// Maximum characters stored per tool output entry (long outputs are truncated).
const MAX_OUTPUT_LENGTH: usize = 100_000;

/// Persist a tool execution result to the session's `tool_outputs.jsonl`.
///
/// Uses atomic append (write line + flush). Returns `Ok(())` on success.
pub async fn append_tool_output(
    session_dir: &Path,
    call_id: &str,
    tool_name: &str,
    args: &Value,
    output: &str,
    is_error: bool,
) -> Result<(), SessionError> {
    let truncated = if output.chars().count() > MAX_OUTPUT_LENGTH {
        let keep = MAX_OUTPUT_LENGTH.saturating_sub(3);
        let mut s: String = output.chars().take(keep).collect();
        s.push_str("...");
        s
    } else {
        output.to_string()
    };

    let entry = ToolOutputEntry {
        call_id: call_id.to_string(),
        tool_name: tool_name.to_string(),
        args: args.clone(),
        output: truncated,
        is_error,
        timestamp: crate::messages::now_iso_timestamp(),
    };

    let line = serde_json::to_string(&entry).map_err(|e| storage_error(session_dir, format!("encode error: {e}")))?;

    let path = session_dir.join(TOOL_OUTPUTS_FILE);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .map_err(|e| storage_error(session_dir, format!("open error: {e}")))?;
    file.write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|e| storage_error(session_dir, format!("write error: {e}")))?;
    file.flush()
        .await
        .map_err(|e| storage_error(session_dir, format!("flush error: {e}")))?;
    Ok(())
}

/// Load all tool output entries from the session directory, newest first.
pub async fn load_tool_outputs(session_dir: &Path) -> Result<Vec<ToolOutputEntry>, SessionError> {
    let path = session_dir.join(TOOL_OUTPUTS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| storage_error(session_dir, format!("read error: {e}")))?;
    let mut entries = Vec::new();
    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<ToolOutputEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                log::warn!("tool_outputs.jsonl: skipping invalid line: {e}");
            }
        }
    }
    Ok(entries)
}

/// Load the most recent N tool output entries, newest first.
pub async fn load_recent_tool_outputs(session_dir: &Path, limit: usize) -> Result<Vec<ToolOutputEntry>, SessionError> {
    let mut entries = load_tool_outputs(session_dir).await?;
    entries.reverse();
    entries.truncate(limit);
    Ok(entries)
}

/// Get tool output for a specific call_id.
pub async fn get_tool_output(session_dir: &Path, call_id: &str) -> Result<Option<ToolOutputEntry>, SessionError> {
    let entries = load_tool_outputs(session_dir).await?;
    Ok(entries.into_iter().find(|e| e.call_id == call_id))
}

fn storage_error(path: &Path, message: impl Into<String>) -> SessionError {
    SessionError::new(
        SessionErrorCode::Storage,
        format!("tool_outputs {}: {}", path.display(), message.into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn test_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    fn test_call_id() -> String {
        format!("t-{}", COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    #[tokio::test]
    async fn append_and_load_tool_output() {
        let dir = test_dir();
        let call_id = test_call_id();
        let args = serde_json::json!({"path": "src/main.rs"});

        append_tool_output(dir.path(), &call_id, "read_file", &args, "fn main() {}", false)
            .await
            .expect("append");

        let entries = load_tool_outputs(dir.path()).await.expect("load");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].call_id, call_id);
        assert_eq!(entries[0].tool_name, "read_file");
        assert_eq!(entries[0].output, "fn main() {}");
        assert!(!entries[0].is_error);
    }

    #[tokio::test]
    async fn append_multiple_and_load_most_recent() {
        let dir = test_dir();
        let args = serde_json::json!({});

        for i in 0..5 {
            append_tool_output(
                dir.path(),
                &format!("t-{i}"),
                "shell_exec",
                &args,
                &format!("output {i}"),
                false,
            )
            .await
            .expect("append");
        }

        let recent = load_recent_tool_outputs(dir.path(), 3).await.expect("load");
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].call_id, "t-4");
        assert_eq!(recent[2].call_id, "t-2");
    }

    #[tokio::test]
    async fn get_tool_output_by_call_id() {
        let dir = test_dir();
        let call_id = test_call_id();
        let args = serde_json::json!({"command": "echo hello"});

        append_tool_output(dir.path(), &call_id, "shell_exec", &args, "hello", false)
            .await
            .expect("append");

        let found = get_tool_output(dir.path(), &call_id)
            .await
            .expect("get")
            .expect("found");
        assert_eq!(found.tool_name, "shell_exec");
        assert_eq!(found.output, "hello");

        let missing = get_tool_output(dir.path(), "nonexistent").await.expect("get");
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn truncates_long_output() {
        let dir = test_dir();
        let call_id = test_call_id();
        let long_output = "x".repeat(MAX_OUTPUT_LENGTH + 1000);
        let args = serde_json::json!({});

        append_tool_output(dir.path(), &call_id, "tool", &args, &long_output, false)
            .await
            .expect("append");

        let entries = load_tool_outputs(dir.path()).await.expect("load");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].output.chars().count() <= MAX_OUTPUT_LENGTH + 3);
        assert!(entries[0].output.ends_with("..."));
    }

    #[tokio::test]
    async fn empty_dir_returns_empty() {
        let dir = test_dir();
        let entries = load_tool_outputs(dir.path()).await.expect("load");
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn records_error_flag() {
        let dir = test_dir();
        let call_id = test_call_id();
        let args = serde_json::json!({"command": "invalid"});

        append_tool_output(dir.path(), &call_id, "shell_exec", &args, "exit code 1", true)
            .await
            .expect("append");

        let entry = get_tool_output(dir.path(), &call_id)
            .await
            .expect("get")
            .expect("found");
        assert!(entry.is_error);
    }
}
