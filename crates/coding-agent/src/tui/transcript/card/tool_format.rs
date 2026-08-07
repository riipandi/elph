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

/// Tighter cap for live streaming thinking so the finalize→collapse transition
/// does not cause a large layout jump (20 lines instead of 48).
pub fn format_thinking_stream_body_display(content: &str) -> String {
    format_process_body_display(content, STREAMING_THINKING_BODY_MAX_LINES, STREAMING_THINKING_BODY_MAX_CHARS)
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
}
