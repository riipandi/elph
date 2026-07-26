//! Tool card argument and output formatting.

pub use crate::tui::tool_params::format_tool_params_display as format_tool_args_display;

pub const TOOL_OUTPUT_MAX_LINES: usize = 12;
pub const TOOL_OUTPUT_MAX_CHARS: usize = 1_500;
/// Cap streaming/expanded thinking body so wrap/layout stay O(viewport), not O(stream).
pub const THINKING_BODY_MAX_LINES: usize = 48;
pub const THINKING_BODY_MAX_CHARS: usize = 3_000;
/// Cap live assistant stream body for layout/render (stable markdown prefix is separate).
pub const ASSISTANT_STREAM_BODY_MAX_LINES: usize = 64;
pub const ASSISTANT_STREAM_BODY_MAX_CHARS: usize = 4_000;

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

/// Keep only the recent tail of a long streaming assistant reply (CPU/memory bound).
pub fn format_assistant_stream_body_display(content: &str) -> String {
    format_process_body_display(content, ASSISTANT_STREAM_BODY_MAX_LINES, ASSISTANT_STREAM_BODY_MAX_CHARS)
}

pub fn format_tool_output_display(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.chars().count() <= TOOL_OUTPUT_MAX_CHARS {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() <= TOOL_OUTPUT_MAX_LINES {
            return trimmed.to_string();
        }
        let mut body = lines
            .iter()
            .take(TOOL_OUTPUT_MAX_LINES)
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        body.push_str(&format!("\n… ({line_count} lines total)", line_count = lines.len()));
        return body;
    }
    let truncated: String = trimmed.chars().take(TOOL_OUTPUT_MAX_CHARS.saturating_sub(1)).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_args_json_single_key_shows_value_only() {
        assert_eq!(format_tool_args_display(r#"{"path":"src/lib.rs"}"#), "src/lib.rs");
    }

    #[test]
    fn tool_output_truncates_long_bodies() {
        let long = "line\n".repeat(20);
        let display = format_tool_output_display(&long);
        assert!(display.contains("lines total"));
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
