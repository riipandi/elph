//! Shared CLI formatting helpers — consistent colors, tables, and structure
//! across all subcommands.
//!
//! All styles respect `NO_COLOR` and auto-detect TTY.

use std::fmt;
use std::io::IsTerminal;

use anstyle::{AnsiColor, Color, Style};

// ── Named styles ─────────────────────────────────────────────────────

pub const S_TITLE: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
pub const S_HEADER: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Blue)));
pub const S_VALUE: Style = Style::new().bold();
pub const S_MUTED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
pub const S_OK: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Green)));
pub const S_WARN: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
pub const S_ERR: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Red)));
pub const S_ACCENT: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
pub const S_BODY: Style = Style::new();
pub const S_TIP: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));

// ── Style context ────────────────────────────────────────────────────

/// Whether to emit ANSI styles (CLI TTY only by default).
#[derive(Debug, Clone, Copy)]
pub struct CliStyle {
    enabled: bool,
}

impl CliStyle {
    pub fn auto() -> Self {
        Self {
            enabled: std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal(),
        }
    }

    pub fn plain() -> Self {
        Self { enabled: false }
    }

    pub fn paint(self, style: Style, text: impl fmt::Display) -> String {
        if !self.enabled {
            return text.to_string();
        }
        format!("{}{}{}", style.render(), text, style.render_reset())
    }
}

// ── Output helpers ───────────────────────────────────────────────────

/// Write a section header with underline.
pub fn section(out: &mut String, sty: CliStyle, title: &str) {
    use std::fmt::Write;
    let _ = writeln!(out, "{}", sty.paint(S_TITLE, title));
    // Use separator line matching terminal width for consistency
    let _ = writeln!(out, "{}", sty.paint(S_MUTED, "────────────────────────────────────────────────────"));
}

/// Write a key-value pair with aligned label.
pub fn kv(out: &mut String, sty: CliStyle, key: &str, value: impl fmt::Display) {
    use std::fmt::Write;
    let _ = writeln!(
        out,
        "  {} {}",
        sty.paint(S_HEADER, format!("{key:<18}")),
        sty.paint(S_VALUE, value)
    );
}

/// Write a simple info line.
pub fn info(out: &mut String, sty: CliStyle, text: impl fmt::Display) {
    use std::fmt::Write;
    let _ = writeln!(out, "  {}", sty.paint(S_BODY, text));
}

/// Write a muted tip line.
pub fn tip(out: &mut String, sty: CliStyle, text: impl fmt::Display) {
    use std::fmt::Write;
    let _ = writeln!(out, "  {}", sty.paint(S_TIP, text));
}

/// Write a success message.
pub fn success(out: &mut String, sty: CliStyle, text: impl fmt::Display) {
    use std::fmt::Write;
    let _ = writeln!(out, "  {}", sty.paint(S_OK, text));
}

/// Write a warning message.
pub fn warn(out: &mut String, sty: CliStyle, text: impl fmt::Display) {
    use std::fmt::Write;
    let _ = writeln!(out, "  {}", sty.paint(S_WARN, text));
}

/// Format a duration in human-readable form.
pub fn fmt_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else {
        format!("{:.1} min", ms as f64 / 60_000.0)
    }
}

/// Format a byte count in human-readable form.
pub fn fmt_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    for unit in UNITS {
        if v < 1024.0 {
            if unit == &"B" {
                return format!("{v:.0} {unit}");
            }
            return format!("{v:.1} {unit}");
        }
        v /= 1024.0;
    }
    format!("{v:.1} GB")
}
