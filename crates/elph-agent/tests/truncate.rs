use elph_agent::harness::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH, TruncatedBy, TruncationOptions, format_size,
    truncate_head, truncate_line, truncate_tail,
};

#[test]
fn format_size_formats_bytes() {
    assert_eq!(format_size(512), "512B");
    assert_eq!(format_size(2048), "2.0KB");
    assert_eq!(format_size(5 * 1024 * 1024), "5.0MB");
}

#[test]
fn truncate_head_returns_original_when_within_limits() {
    let content = "line one\nline two\nline three";
    let result = truncate_head(content, TruncationOptions::default());
    assert!(!result.truncated);
    assert_eq!(result.content, content);
    assert_eq!(result.total_lines, 3);
}

#[test]
fn truncate_head_limits_by_line_count() {
    let content = (0..10).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    let result = truncate_head(
        &content,
        TruncationOptions {
            max_lines: Some(3),
            max_bytes: Some(DEFAULT_MAX_BYTES),
        },
    );
    assert!(result.truncated);
    assert_eq!(result.truncated_by, Some(TruncatedBy::Lines));
    assert_eq!(result.output_lines, 3);
    assert_eq!(result.content, "line 0\nline 1\nline 2");
}

#[test]
fn truncate_head_limits_by_byte_count() {
    let content = "short\nanother line\nthird";
    let result = truncate_head(
        content,
        TruncationOptions {
            max_lines: Some(DEFAULT_MAX_LINES),
            max_bytes: Some(12),
        },
    );
    assert!(result.truncated);
    assert_eq!(result.truncated_by, Some(TruncatedBy::Bytes));
    assert_eq!(result.content, "short");
}

#[test]
fn truncate_head_reports_first_line_exceeds_limit() {
    let content = "x".repeat(100);
    let result = truncate_head(
        &content,
        TruncationOptions {
            max_lines: Some(DEFAULT_MAX_LINES),
            max_bytes: Some(10),
        },
    );
    assert!(result.truncated);
    assert!(result.first_line_exceeds_limit);
    assert!(result.content.is_empty());
}

#[test]
fn truncate_tail_returns_original_when_within_limits() {
    let content = "alpha\nbeta\ngamma";
    let result = truncate_tail(content, TruncationOptions::default());
    assert!(!result.truncated);
    assert_eq!(result.content, content);
}

#[test]
fn truncate_tail_keeps_last_lines() {
    let content = (0..10).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    let result = truncate_tail(
        &content,
        TruncationOptions {
            max_lines: Some(3),
            max_bytes: Some(DEFAULT_MAX_BYTES),
        },
    );
    assert!(result.truncated);
    assert_eq!(result.truncated_by, Some(TruncatedBy::Lines));
    assert_eq!(result.content, "line 7\nline 8\nline 9");
}

#[test]
fn truncate_tail_can_return_partial_line() {
    let content = "x".repeat(30);
    let result = truncate_tail(
        &content,
        TruncationOptions {
            max_lines: Some(1),
            max_bytes: Some(10),
        },
    );
    assert!(result.truncated);
    assert!(result.last_line_partial);
    assert_eq!(result.output_bytes, 10);
}

#[test]
fn truncate_line_adds_suffix_when_needed() {
    let line = "a".repeat(GREP_MAX_LINE_LENGTH + 10);
    let (text, was_truncated) = truncate_line(&line, GREP_MAX_LINE_LENGTH);
    assert!(was_truncated);
    assert!(text.ends_with("... [truncated]"));
}

#[test]
fn truncate_line_keeps_short_lines() {
    let (text, was_truncated) = truncate_line("short line", GREP_MAX_LINE_LENGTH);
    assert!(!was_truncated);
    assert_eq!(text, "short line");
}
