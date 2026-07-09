//! Shared formatting for tool execution output in the Owly TUI.

use elph_tui::{ToolExecutionState, ToolExecutionStatus};

const TRANSCRIPT_INDENT: &str = "      ";

/// Icon for a tool row in the transcript.
pub fn tool_status_icon(status: ToolExecutionStatus) -> &'static str {
    match status {
        ToolExecutionStatus::Success => "✓",
        ToolExecutionStatus::Error => "✗",
        ToolExecutionStatus::Cancelled => "⊘",
        ToolExecutionStatus::Running => "⠋",
        ToolExecutionStatus::Pending => "○",
    }
}

/// Header line: `  ✓ tool_name` (args/output rendered separately, untruncated).
pub fn tool_transcript_header(tool: &ToolExecutionState) -> String {
    format!("  {} {}", tool_status_icon(tool.status), tool.name)
}

/// Full args + output block for the chat transcript (no truncation).
pub fn tool_transcript_body(tool: &ToolExecutionState) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(block) = indent_block(tool.args_summary.trim(), TRANSCRIPT_INDENT) {
        parts.push(block);
    }
    if let Some(block) = indent_block(tool.output.trim(), TRANSCRIPT_INDENT) {
        parts.push(block);
    }
    if parts.is_empty() { None } else { Some(parts.join("\n")) }
}

fn indent_block(text: &str, prefix: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let lines: Vec<String> = text.lines().map(|line| format!("{prefix}{line}")).collect();
    if lines.is_empty() { None } else { Some(lines.join("\n")) }
}

/// First-line preview of tool output for compact activity-bar chips.
pub fn tool_output_preview(output: &str, max_chars: usize) -> Option<String> {
    let max_chars = max_chars.max(8);
    let line = output.trim().lines().find(|line| !line.trim().is_empty())?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_chars(trimmed, max_chars))
}

/// Compact chip label for the activity bar (truncated; transcript uses [`tool_transcript_body`]).
pub fn tool_chip_label(tool: &ToolExecutionState, args_max: usize, preview_max: usize) -> String {
    let args = tool.args_summary.trim();
    let base = if args.is_empty() {
        tool.name.clone()
    } else {
        format!("{} {}", tool.name, truncate_chars(args, args_max))
    };
    if let Some(preview) = tool_output_preview(&tool.output, preview_max) {
        format!("{base} → {preview}")
    } else {
        base
    }
}

pub fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let end = value
            .char_indices()
            .nth(max.saturating_sub(1))
            .map(|(i, _)| i)
            .unwrap_or(value.len());
        format!("{}…", &value[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_tui::{ToolExecutionState, ToolExecutionStatus};

    #[test]
    fn transcript_body_includes_full_args_and_output() {
        let tool = ToolExecutionState::new("1", "bash")
            .with_args(r#"{"command":"cargo test -p owly --all-targets"}"#)
            .with_output("running 204 tests\nall passed")
            .with_status(ToolExecutionStatus::Success);
        let body = tool_transcript_body(&tool).expect("body");
        assert!(body.contains(r#"{"command":"cargo test -p owly --all-targets"}"#));
        assert!(body.contains("running 204 tests"));
        assert!(body.contains("all passed"));
        assert!(!body.contains('…'));
    }

    #[test]
    fn transcript_header_is_name_only() {
        let tool = ToolExecutionState::new("1", "read")
            .with_args(r#"{"path":"src/lib.rs"}"#)
            .with_status(ToolExecutionStatus::Success);
        assert_eq!(tool_transcript_header(&tool), "  ✓ read");
    }

    #[test]
    fn preview_uses_first_non_empty_line() {
        let preview = tool_output_preview("\n\nWrote 42 bytes\n", 20).expect("preview");
        assert_eq!(preview, "Wrote 42 bytes");
    }

    #[test]
    fn chip_includes_output_arrow() {
        let tool = ToolExecutionState::new("1", "write")
            .with_args(r#"{"path":"a.md"}"#)
            .with_output("Wrote 120 bytes to a.md")
            .with_status(ToolExecutionStatus::Success);
        let label = tool_chip_label(&tool, 24, 32);
        assert!(label.contains("write"));
        assert!(label.contains("→ Wrote 120 bytes"));
    }
}
