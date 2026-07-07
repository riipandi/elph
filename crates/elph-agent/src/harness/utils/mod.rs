//! Harness utility helpers — ported from pi-agent `harness/utils/`.

pub mod shell_output;
pub mod truncate;

pub use shell_output::{ShellCaptureResult, finalize_shell_capture, sanitize_binary_output};
pub use truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH, TruncatedBy, TruncationOptions, TruncationResult,
    format_size, truncate_head, truncate_line, truncate_tail,
};
