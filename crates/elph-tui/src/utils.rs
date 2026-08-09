//! Text layout helpers.

use std::iter::Peekable;
use std::str::Chars;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Normalize user prompt text before sticky-card wrap/clamp so width math and rendering stay clean.
pub fn sanitize_sticky_display_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => skip_ansi_tail(&mut chars),
            '\u{009b}' => skip_csi_params(&mut chars),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\n' => out.push('\n'),
            '\t' => out.push(' '),
            c if c.is_control() => {}
            c if is_invisible_format_char(c) => {}
            c => out.push(c),
        }
    }
    out
}

fn skip_ansi_tail(chars: &mut Peekable<Chars<'_>>) {
    match chars.next() {
        Some('[') => skip_csi_params(chars),
        Some(']') => {
            while let Some(ch) = chars.next() {
                if ch == '\x07' || ch == '\u{009c}' {
                    break;
                }
                if ch == '\x1b' && chars.next() == Some('\\') {
                    break;
                }
            }
        }
        Some('(') | Some(')') | Some('#') => {
            chars.next();
        }
        Some(_) => {}
        None => {}
    }
}

fn skip_csi_params(chars: &mut Peekable<Chars<'_>>) {
    for ch in chars.by_ref() {
        if ('@'..='~').contains(&ch) {
            break;
        }
    }
}

fn is_invisible_format_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{00ad}' // soft hyphen
            | '\u{034f}' // CGJ
            | '\u{061c}' // ALM
            | '\u{180e}' // Mongolian vowel separator
            | '\u{200b}'..='\u{200f}' // ZWSP, ZWNJ, ZWJ, LRM, RLM
            | '\u{2028}'..='\u{2029}' // line/paragraph separators → drop (sticky uses explicit \n)
            | '\u{202a}'..='\u{202e}' // bidi embedding controls
            | '\u{2060}'..='\u{2064}' // word joiner, invisible operators
            | '\u{206a}'..='\u{206f}' // deprecated format controls
            | '\u{feff}' // BOM / ZWNBSP
            | '\u{fff9}'..='\u{fffb}' // interlinear annotation anchors
    )
}

/// Compact, auto-scaled duration for transcript / process-log / activity chrome.
///
/// Smallest unit is **nanoseconds** so sub-millisecond work never collapses to `0ms` / `0s`.
///
/// | Range            | Format   | Example              |
/// |------------------|----------|----------------------|
/// | &lt; 1µs         | ns       | `420ns`, `0ns`       |
/// | 1µs – &lt;1ms    | us       | `12us`, `850us`      |
/// | 1ms – &lt;1s     | ms       | `1ms`, `45ms`        |
/// | 1s – &lt;10s     | tenths s | `1.2s`, `9.9s`       |
/// | 10s – &lt;1m     | whole s  | `12s`, `59s`         |
/// | 1m – &lt;1h      | m[+s]    | `1m`, `1m30s`        |
/// | ≥ 1h             | h[+m][+s]| `1h`, `1h2m5s`       |
pub fn format_duration_secs(elapsed_secs: f64) -> String {
    let secs = if elapsed_secs.is_finite() {
        elapsed_secs.max(0.0)
    } else {
        0.0
    };

    // Sub-millisecond: us, or ns when under 1µs (never "0ms").
    if secs < 0.001 {
        let ns = (secs * 1_000_000_000.0).round() as u64;
        if ns < 1_000 {
            return format!("{ns}ns");
        }
        let us = (secs * 1_000_000.0).round() as u64;
        if us == 0 {
            return format!("{ns}ns");
        }
        return format!("{us}us");
    }

    // Under 1s: integer milliseconds; fall back to us/ns if rounding would print 0ms.
    if secs < 1.0 {
        let ms = (secs * 1000.0).round() as u64;
        if ms == 0 {
            let us = (secs * 1_000_000.0).round() as u64;
            if us == 0 {
                let ns = (secs * 1_000_000_000.0).round() as u64;
                return format!("{ns}ns");
            }
            return format!("{us}us");
        }
        return format!("{ms}ms");
    }

    // Under 10s: one decimal second (drops trailing `.0`).
    if secs < 10.0 {
        let rounded_tenth = (secs * 10.0).round() / 10.0;
        let whole = rounded_tenth.floor();
        if (rounded_tenth - whole).abs() < 0.05 {
            return format!("{}s", whole as u64);
        }
        return format!("{rounded_tenth:.1}s");
    }

    // 10s–59s: whole seconds.
    if secs < 60.0 {
        return format!("{}s", secs.round() as u64);
    }

    let total = secs.round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    if hours > 0 {
        if seconds > 0 {
            format!("{hours}h{minutes}m{seconds}s")
        } else if minutes > 0 {
            format!("{hours}h{minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if seconds > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{minutes}m")
    }
}

/// Word-wrap plain text to fit `max_width` display columns.
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0;

        for word in paragraph.split_whitespace() {
            let word_width = word.width();
            let extra = if current.is_empty() { 0 } else { 1 };
            if current_width + extra + word_width > max_width {
                if !current.is_empty() {
                    lines.push(current);
                    current = String::new();
                    current_width = 0;
                }
                if word_width > max_width {
                    for chunk in chunk_graphemes(word, max_width) {
                        lines.push(chunk);
                    }
                    continue;
                }
            }
            if !current.is_empty() {
                current.push(' ');
                current_width += 1;
            }
            current.push_str(word);
            current_width += word_width;
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn chunk_graphemes(text: &str, max_width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut width = 0;

    for g in text.graphemes(true) {
        let g_width = g.width();
        if width + g_width > max_width && !current.is_empty() {
            chunks.push(current);
            current = String::new();
            width = 0;
        }
        current.push_str(g);
        width += g_width;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Display-column width of `text` (grapheme-aware via unicode-width).
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Truncate text to `max_width` display columns with an ellipsis suffix.
pub fn truncate_with_ellipsis(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width <= 1 {
        return "…".to_string();
    }

    let target = max_width - 1;
    let mut out = String::new();
    let mut width = 0;
    for g in text.graphemes(true) {
        let g_width = g.width();
        if width + g_width > target {
            break;
        }
        out.push_str(g);
        width += g_width;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_sticky_display_text_strips_ansi_and_controls() {
        let raw = "\x1b[31mhello\x07world\x1b[0m";
        assert_eq!(sanitize_sticky_display_text(raw), "helloworld");
    }

    #[test]
    fn sanitize_sticky_display_text_normalizes_newlines_and_tabs() {
        assert_eq!(sanitize_sticky_display_text("a\r\nb\rc\td"), "a\nb\nc d");
    }

    #[test]
    fn sanitize_sticky_display_text_drops_zero_width_chars() {
        let raw = "hel\u{200b}lo\u{feff}";
        assert_eq!(sanitize_sticky_display_text(raw), "hello");
    }

    #[test]
    fn sanitize_sticky_display_text_is_idempotent() {
        let once = sanitize_sticky_display_text("plain\nline");
        assert_eq!(sanitize_sticky_display_text(&once), once);
    }

    #[test]
    fn format_duration_secs_uses_ns_and_us_under_one_ms() {
        assert_eq!(format_duration_secs(0.0), "0ns");
        assert_eq!(format_duration_secs(0.000_000_42), "420ns");
        assert_eq!(format_duration_secs(0.000_012), "12us");
        assert_eq!(format_duration_secs(0.000_85), "850us");
    }

    #[test]
    fn format_duration_secs_never_prints_zero_ms_or_zero_s() {
        // Values that would round to 0ms under a ms-only scale.
        assert_ne!(format_duration_secs(0.000_4), "0ms");
        assert!(format_duration_secs(0.000_4).ends_with("us") || format_duration_secs(0.000_4).ends_with("ns"));
        assert_ne!(format_duration_secs(0.0), "0ms");
        assert_ne!(format_duration_secs(0.0), "0s");
    }

    #[test]
    fn format_duration_secs_uses_ms_under_one_second() {
        assert_eq!(format_duration_secs(0.045), "45ms");
        assert_eq!(format_duration_secs(0.5), "500ms");
        assert_eq!(format_duration_secs(0.999), "999ms");
    }

    #[test]
    fn format_duration_secs_scales_up_through_hours() {
        assert_eq!(format_duration_secs(1.0), "1s");
        assert_eq!(format_duration_secs(1.24), "1.2s");
        assert_eq!(format_duration_secs(12.0), "12s");
        assert_eq!(format_duration_secs(90.0), "1m30s");
        assert_eq!(format_duration_secs(3600.0), "1h");
    }
}
