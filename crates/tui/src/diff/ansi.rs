//! ANSI styling helpers for diff-TUI components.

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const ITALIC: &str = "\x1b[3m";
pub const UNDERLINE: &str = "\x1b[4m";
pub const STRIKE: &str = "\x1b[9m";
pub const DIM: &str = "\x1b[2m";

/// 256-color foreground SGR.
pub fn fg(color: u8) -> String {
    format!("\x1b[38;5;{color}m")
}

/// 256-color background SGR.
#[allow(dead_code)]
pub fn bg(color: u8) -> String {
    format!("\x1b[48;5;{color}m")
}

/// Wraps `text` with `prefix` and [`RESET`].
pub fn styled(prefix: &str, text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    format!("{prefix}{text}{RESET}")
}

/// OSC-8 hyperlink (supported by iTerm2, Kitty, WezTerm, etc.).
pub fn hyperlink(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// Active inline style prefix to re-apply after nested styling resets.
#[derive(Debug, Default, Clone)]
pub struct StylePrefix {
    parts: Vec<String>,
}

impl StylePrefix {
    pub fn push(&mut self, part: impl Into<String>) {
        self.parts.push(part.into());
    }

    pub fn as_str(&self) -> String {
        self.parts.join("")
    }

    pub fn apply_after(&self, styled: &str) -> String {
        if self.parts.is_empty() {
            styled.to_string()
        } else {
            format!("{styled}{}", self.as_str())
        }
    }

    pub fn pop(&mut self) {
        self.parts.pop();
    }
}
