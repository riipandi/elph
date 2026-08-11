//! Colored terminal output for `generate-models`.
//!
//! Respects `NO_COLOR` and non-TTY stdout (plain text when redirected).

use std::fmt;
use std::io::{self, IsTerminal};

use anstyle::{AnsiColor, Color, Style};

const GREEN: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const YELLOW: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
const RED: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Red)));
const CYAN: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const BLUE: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Blue)));
const MUTED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
const BOLD: Style = Style::new().bold();
const MAGENTA: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Magenta)));

fn color_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn paint(style: Style, text: impl fmt::Display) -> String {
    if color_enabled() {
        format!("{}{}{}", style.render(), text, style.render_reset())
    } else {
        text.to_string()
    }
}

/// Dim step / status line.
pub fn info(msg: impl fmt::Display) {
    println!("  {}", paint(MUTED, msg));
}

/// Fetch / network progress.
pub fn fetch(msg: impl fmt::Display) {
    println!("  {}", paint(CYAN, msg));
}

/// Successful provider write: green id + muted detail.
pub fn provider_ok(provider_id: &str, count: usize, file_name: &str) {
    if color_enabled() {
        println!(
            "  {} {} {} {} {}",
            paint(GREEN, "✓"),
            paint(BOLD, provider_id),
            paint(MUTED, "→"),
            paint(GREEN, format!("{count} models")),
            paint(MUTED, format!("({file_name})")),
        );
    } else {
        println!("  ✓ {provider_id} → {count} models ({file_name})");
    }
}

/// Warning / skipped provider.
pub fn warn(msg: impl fmt::Display) {
    if color_enabled() {
        println!("  {} {}", paint(YELLOW, "!"), paint(YELLOW, msg));
    } else {
        println!("  ! {msg}");
    }
}

/// Hard error style (for messages before bail).
pub fn err(msg: impl fmt::Display) {
    if color_enabled() {
        eprintln!("  {} {}", paint(RED, "✗"), paint(RED, msg));
    } else {
        eprintln!("  ✗ {msg}");
    }
}

/// Final success summary.
pub fn success(msg: impl fmt::Display) {
    if color_enabled() {
        println!("\n{} {}", paint(GREEN, "✓"), paint(BOLD, msg));
    } else {
        println!("\n✓ {msg}");
    }
}

/// Section header.
pub fn header(msg: impl fmt::Display) {
    if color_enabled() {
        println!("\n{}", paint(BLUE, msg));
    } else {
        println!("\n{msg}");
    }
}

/// Thinking-map / metric line.
pub fn metric(label: &str, ok: usize, bad: usize) {
    if color_enabled() {
        let ok_s = paint(GREEN, format!("complete={ok}"));
        let bad_s = if bad > 0 {
            paint(RED, format!("incomplete={bad}"))
        } else {
            paint(MUTED, format!("incomplete={bad}"))
        };
        println!("  {} {}  {}", paint(MAGENTA, label), ok_s, bad_s);
    } else {
        println!("  {label}: complete={ok} incomplete={bad}");
    }
}

/// Live pricing hit.
pub fn live_pricing(provider_id: &str, count: usize) {
    if color_enabled() {
        println!(
            "  {} {} {}",
            paint(CYAN, "↗"),
            paint(BOLD, provider_id),
            paint(MUTED, format!("{count} live prices")),
        );
    } else {
        println!("  Live pricing {provider_id}: {count} models");
    }
}

/// Dim note (e.g. skipped probes).
pub fn note(msg: impl fmt::Display) {
    println!("  {}", paint(MUTED, msg));
}

/// Verification OK.
pub fn verified(msg: impl fmt::Display) {
    if color_enabled() {
        println!("  {} {}", paint(GREEN, "✓"), paint(MUTED, msg));
    } else {
        println!("  ✓ {msg}");
    }
}

/// thinkingLevelMap source breakdown summary line.
pub fn source_breakdown(
    previous: usize,
    live_api: usize,
    models_dev: usize,
    provider_override: usize,
    unresolved: usize,
) {
    let total = previous + live_api + models_dev + provider_override + unresolved;
    if total == 0 {
        return;
    }
    let parts: Vec<String> = [
        (live_api, "live-api"),
        (models_dev, "models.dev"),
        (provider_override, "provider-override"),
        (previous, "previous"),
    ]
    .iter()
    .filter(|(c, _)| *c > 0)
    .map(|(c, s)| format!("{s}={c}"))
    .collect();
    let parts_str = parts.join(" ");
    if color_enabled() {
        if unresolved > 0 {
            println!(
                "  {} {}  {}",
                paint(MAGENTA, "thinkingLevelMap source"),
                paint(YELLOW, format!("unresolved={unresolved}")),
                paint(MUTED, parts_str),
            );
        } else {
            println!(
                "  {} {} {}",
                paint(MAGENTA, "thinkingLevelMap source"),
                paint(MUTED, parts_str),
                paint(GREEN, format!("unresolved={unresolved}")),
            );
        }
    } else {
        let unresolved_s = if unresolved > 0 {
            format!(" unresolved={unresolved}")
        } else {
            String::new()
        };
        println!("  thinkingLevelMap source: {parts_str}{unresolved_s}");
    }
}

/// Resolved cost source breakdown summary line.
pub fn cost_breakdown(live_api: usize, models_dev: usize, aimd: usize, previous: usize, none: usize) {
    let total = live_api + models_dev + aimd + previous + none;
    if total == 0 {
        return;
    }
    let parts: Vec<String> = [
        (live_api, "live-api"),
        (models_dev, "models.dev"),
        (aimd, "ai-model-directory"),
        (previous, "previous"),
    ]
    .iter()
    .filter(|(c, _)| *c > 0)
    .map(|(c, s)| format!("{s}={c}"))
    .collect();
    let parts_str = parts.join(" ");
    if color_enabled() {
        if none > 0 {
            println!(
                "  {} {}  {}",
                paint(MAGENTA, "cost source"),
                paint(YELLOW, format!("none={none}")),
                paint(MUTED, parts_str),
            );
        } else {
            println!(
                "  {} {} {}",
                paint(MAGENTA, "cost source"),
                paint(MUTED, parts_str),
                paint(GREEN, format!("none={none}")),
            );
        }
    } else {
        let none_s = if none > 0 {
            format!(" none={none}")
        } else {
            String::new()
        };
        println!("  cost source: {parts_str}{none_s}");
    }
}
