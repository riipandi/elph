//! Raw terminal input helpers for diff-TUI components.

/// Returns true for arrow-up sequences.
pub fn is_up(data: &str) -> bool {
    matches!(data, "\x1b[A" | "\x1bOA" | "\x1b[1;2A" | "\x1b[1;3A")
}

/// Returns true for arrow-down sequences.
pub fn is_down(data: &str) -> bool {
    matches!(data, "\x1b[B" | "\x1bOB" | "\x1b[1;2B" | "\x1b[1;3B")
}

/// Returns true for Enter.
pub fn is_enter(data: &str) -> bool {
    matches!(data, "\r" | "\n" | "\x1b\r" | "\x1b\n")
}

/// Returns true for Escape or Ctrl+C cancel sequences.
pub fn is_cancel(data: &str) -> bool {
    matches!(data, "\x1b" | "\x1b\x1b" | "\x03" | "\x1b[3;5~")
}
