//! Tool card argument and output formatting.

pub use crate::tui::tool_params::format_tool_params_display as format_tool_args_display;

pub const TOOL_OUTPUT_MAX_LINES: usize = 8;
pub const TOOL_OUTPUT_MAX_CHARS: usize = 1_500;
/// Cap streaming/expanded thinking body so wrap/layout stay O(viewport), not O(stream).
pub const THINKING_BODY_MAX_LINES: usize = 48;
pub const THINKING_BODY_MAX_CHARS: usize = 3_000;
/// Cap live assistant stream body for layout/render (stable markdown prefix is separate).
pub const ASSISTANT_STREAM_BODY_MAX_LINES: usize = 16;
pub const ASSISTANT_STREAM_BODY_MAX_CHARS: usize = 4_000;
/// Streaming thinking body cap — tighter than the finished-expanded limit so live
/// deltas stay bounded and the collapse-on-finish transition does not flicker.
///
/// Together with the thinking header (`◌ Thinking · running` = 1 row), the header↔body
/// flex gap (1 row) and the streaming body (≤ 8 rows), a live thinking card never exceeds
/// 10 rows — the fixed max height during streaming.
pub const STREAMING_THINKING_BODY_MAX_LINES: usize = 8;
pub const STREAMING_THINKING_BODY_MAX_CHARS: usize = 1_500;

/// Truncate long process-phase bodies for display + row measurement.
pub fn format_process_body_display(content: &str, max_lines: usize, max_chars: usize) -> String {
    let trimmed = content.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = trimmed.lines().collect();
    let body = if lines.len() > max_lines {
        let skip = lines.len().saturating_sub(max_lines);
        format!("… ({skip} earlier lines)\n{}", lines[skip..].join("\n"))
    } else {
        trimmed.to_string()
    };
    if body.chars().count() <= max_chars {
        return body;
    }
    let keep = max_chars.saturating_sub(1).max(1);
    let truncated: String = body.chars().take(keep).collect();
    format!("{truncated}…")
}

pub fn format_thinking_body_display(content: &str) -> String {
    format_process_body_display(content, THINKING_BODY_MAX_LINES, THINKING_BODY_MAX_CHARS)
}

pub use elph_tui::word_wrap::wrap_text_to_lines;

/// Cap a live streaming thinking body to a **fixed number of wrapped rows** so the card
/// keeps a stable height while the model streams more text.
///
/// The cap is measured in *wrapped* rows at the exact inner width the card paints at
/// (mirroring iocraft via [`wrap_text_to_lines`]), so a single long paragraph — one source
/// line that wraps to many painted rows — cannot inflate the transcript height row by row.
/// The old cap truncated by source line, which let exactly that case grow without limit.
///
/// The body keeps the **recent tail** (newest rows) and prefixes the dropped portion with
/// `… (N earlier lines)` where N counts wrapped rows — the transcript then shows the latest
/// reasoning while staying at most [`STREAMING_THINKING_BODY_MAX_LINES`] rows tall. Short
/// streaming thinking is returned untouched (adaptive: the box shrinks when thinking is
/// short, and only locks to full height once the content would pass the cap).
///
/// The prefix line itself wraps at narrow widths, so the tail is shed until prefix + tail
/// fits the row cap — the painted card never exceeds the fixed max height.
pub fn format_thinking_stream_body_display(content: &str, wrap_width: u16) -> String {
    let trimmed = content.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }
    let wrap_width = usize::from(wrap_width.max(1));
    // Wrap the whole body exactly like iocraft paints it, then keep the newest rows.
    let rows = wrap_text_to_lines(trimmed, wrap_width);
    let total = rows.len();
    let mut tail_len = total.min(STREAMING_THINKING_BODY_MAX_LINES);
    let mut dropped = total.saturating_sub(tail_len);
    loop {
        // `wrap_text_to_lines` keeps trailing whitespace incl. `\n` inside a segment (mirroring
        // how iocraft's wrap emits it); strip it before re-joining so the multi-line body does
        // not gain phantom blank rows, while still matching iocraft's painted row count.
        let tail = rows[total.saturating_sub(tail_len)..]
            .iter()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");
        let body = if dropped > 0 {
            format!("… ({dropped} earlier lines)\n{tail}")
        } else {
            tail
        };
        if wrapped_rows_of(&body, wrap_width) <= STREAMING_THINKING_BODY_MAX_LINES {
            return body;
        }
        if dropped == total {
            break; // Pathological width: even the bare prefix over-wraps — return as-is.
        }
        tail_len = tail_len.saturating_sub(1);
        dropped += 1;
    }
    // Last-resort char cap (keeps the newest characters). Only reachable when the row cap
    // was already met, so this cannot grow the painted height.
    if trimmed.chars().count() > STREAMING_THINKING_BODY_MAX_CHARS {
        let keep = STREAMING_THINKING_BODY_MAX_CHARS.saturating_sub(1).max(1);
        let truncated: String = trimmed
            .chars()
            .rev()
            .take(keep)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return format!("…{truncated}");
    }
    trimmed.to_string()
}

/// Painted row count of `text` at `wrap_width` — mirrors what iocraft renders.
fn wrapped_rows_of(text: &str, wrap_width: usize) -> usize {
    wrap_text_to_lines(text, wrap_width).len()
}

/// Keep only the recent tail of a long streaming assistant reply (CPU/memory bound).
pub fn format_assistant_stream_body_display(content: &str) -> String {
    format_process_body_display(content, ASSISTANT_STREAM_BODY_MAX_LINES, ASSISTANT_STREAM_BODY_MAX_CHARS)
}

/// Full tool output for finished expanded cards — no truncation so the user
/// sees the complete result when they expand a settled tool card.
pub fn format_tool_output_display_full(output: &str) -> String {
    let sanitized = sanitize_tool_body(output);
    sanitized.trim().to_string()
}

pub fn format_tool_output_display(output: &str) -> String {
    // Drop bare CR / other C0 controls that web pages often embed — raw-mode TUIs
    // treat `\r` as cursor home and corrupt the frame until a full redraw (resize).
    let sanitized = sanitize_tool_body(output);
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = trimmed.lines().collect();
    if lines.len() <= TOOL_OUTPUT_MAX_LINES && trimmed.chars().count() <= TOOL_OUTPUT_MAX_CHARS {
        return trimmed.to_string();
    }
    // Show tail (last N lines) so streaming output stays visible at the bottom.
    let body = if lines.len() > TOOL_OUTPUT_MAX_LINES {
        let skip = lines.len().saturating_sub(TOOL_OUTPUT_MAX_LINES);
        let tail = lines[skip..].join("\n");
        format!("… ({skip} lines before this)\n{tail}")
    } else {
        trimmed.to_string()
    };
    if body.chars().count() <= TOOL_OUTPUT_MAX_CHARS {
        return body;
    }
    let keep = TOOL_OUTPUT_MAX_CHARS.saturating_sub(1).max(1);
    let truncated: String = body.chars().take(keep).collect();
    format!("{truncated}…")
}

/// Render full tool output without any truncation — used for user-initiated shell (`!`/`!!`).
pub fn format_tool_output_display_unlimited(output: &str) -> String {
    let sanitized = sanitize_tool_body(output);
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        trimmed.to_string()
    }
}

fn sanitize_tool_body(output: &str) -> String {
    output
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_args_json_single_key_shows_value_only() {
        assert_eq!(format_tool_args_display(r#"{"path":"src/lib.rs"}"#), "src/lib.rs");
    }

    #[test]
    fn tool_output_shows_tail_with_count() {
        let long = "line\n".repeat(30);
        let display = format_tool_output_display(&long);
        assert!(display.contains("lines before this"), "{display}");
        // Tail: should contain the last few lines
        assert!(display.contains("line\nline\nline\nline\nline"), "{display}");
    }

    #[test]
    fn tool_output_strips_carriage_returns_from_web_bodies() {
        let dirty = "title\r\nhttps://example.com/path\rnext";
        let display = format_tool_output_display(dirty);
        assert!(!display.contains('\r'), "{display:?}");
        assert!(display.contains("https://example.com/path"));
        assert!(display.contains("next"));
    }

    #[test]
    fn thinking_body_keeps_recent_tail() {
        let long = (0..80).map(|i| format!("think {i}")).collect::<Vec<_>>().join("\n");
        let display = format_thinking_body_display(&long);
        assert!(display.contains("earlier lines"), "{display}");
        assert!(display.contains("think 79"), "{display}");
        assert!(!display.contains("think 0\n"), "{display}");
    }

    #[test]
    fn streaming_thinking_wraps_cap_keeps_tail_within_limit() {
        // One source line that wraps to many painted rows must be capped by *wrapped* rows.
        let long = (0..200).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
        for width in [20u16, 40, 80] {
            let display = format_thinking_stream_body_display(&long, width);
            let rows = wrapped_rows_of(&display, usize::from(width));
            assert!(
                rows <= STREAMING_THINKING_BODY_MAX_LINES,
                "width {width}: wrapped rows {rows} > cap {} for {display:?}",
                STREAMING_THINKING_BODY_MAX_LINES
            );
            // The very latest token must survive truncation.
            assert!(display.contains("word199"), "width {width}: last token lost in {display:?}");
            // Truncation must be visible.
            assert!(
                display.starts_with('…'),
                "width {width}: expected truncation prefix in {display:?}"
            );
        }
    }

    #[test]
    fn streaming_thinking_short_body_stays_untouched() {
        let short = "First thought\nsecond thought";
        assert_eq!(format_thinking_stream_body_display(short, 80), short);
    }

    #[test]
    fn streaming_thinking_many_source_lines_shows_earlier_count() {
        let long = (0..60).map(|i| format!("think {i}")).collect::<Vec<_>>().join("\n");
        let display = format_thinking_stream_body_display(&long, 40);
        assert!(display.contains("earlier lines"), "{display}");
        assert!(display.contains("think 59"), "{display}");
        assert!(
            wrapped_rows_of(&display, 40) <= STREAMING_THINKING_BODY_MAX_LINES,
            "{display:?}"
        );
        assert!(!display.contains("think 0"), "{display:?}");
    }

    #[test]
    fn streaming_thinking_empty_is_empty() {
        assert_eq!(format_thinking_stream_body_display("", 40), "");
        assert_eq!(format_thinking_stream_body_display("   \n  ", 40), "");
    }
}
