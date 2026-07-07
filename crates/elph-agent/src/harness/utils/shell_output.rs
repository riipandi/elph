//! Shell output capture helpers — ported from pi-agent `harness/utils/shell-output.ts`.

use crate::harness::types::{ExecutionError, ExecutionErrorCode, Result};
use crate::harness::utils::truncate::{TruncationOptions, truncate_tail};

/// Result of capturing shell command output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCaptureResult {
    pub output: String,
    pub exit_code: Option<i32>,
    pub cancelled: bool,
    pub truncated: bool,
    pub full_output_path: Option<String>,
}

/// Remove control characters and invalid Unicode from shell output.
pub fn sanitize_binary_output(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            let code = *ch as u32;
            if code == 0x09 || code == 0x0a || code == 0x0d {
                return true;
            }
            if code <= 0x1f {
                return false;
            }
            if (0xfff9..=0xfffb).contains(&code) {
                return false;
            }
            true
        })
        .collect()
}

/// Sanitize and truncate captured shell output from the tail.
pub fn finalize_shell_capture(output: &str, options: Option<TruncationOptions>) -> ShellCaptureResult {
    let sanitized = sanitize_binary_output(output).replace('\r', "");
    let truncation = truncate_tail(&sanitized, options.unwrap_or_default());
    ShellCaptureResult {
        output: truncation.content,
        exit_code: None,
        cancelled: false,
        truncated: truncation.truncated,
        full_output_path: None,
    }
}

pub fn to_execution_error(error: impl std::fmt::Display) -> ExecutionError {
    ExecutionError::new(ExecutionErrorCode::Unknown, error.to_string())
}

pub fn ok_shell_capture(result: ShellCaptureResult) -> Result<ShellCaptureResult, ExecutionError> {
    crate::harness::types::ok(result)
}
