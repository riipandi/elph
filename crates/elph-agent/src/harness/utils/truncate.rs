//! Shared truncation utilities for tool outputs — ported from pi-agent `harness/utils/truncate.ts`.
//!
//! Truncation is based on two independent limits — whichever is hit first wins:
//! - Line limit (default: 2000 lines)
//! - Byte limit (default: 50KB)
//!
//! Never returns partial lines (except bash tail truncation edge case).

/// Default maximum number of lines before truncation.
pub const DEFAULT_MAX_LINES: usize = 2000;
/// Default maximum number of bytes before truncation.
pub const DEFAULT_MAX_BYTES: usize = 50 * 1024;
/// Maximum characters per grep match line.
pub const GREP_MAX_LINE_LENGTH: usize = 500;

/// Result of truncating content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationResult {
    /// The truncated content.
    pub content: String,
    /// Whether truncation occurred.
    pub truncated: bool,
    /// Which limit was hit: `"lines"`, `"bytes"`, or `None` if not truncated.
    pub truncated_by: Option<TruncatedBy>,
    /// Total number of lines in the original content.
    pub total_lines: usize,
    /// Total number of bytes in the original content.
    pub total_bytes: usize,
    /// Number of complete lines in the truncated output.
    pub output_lines: usize,
    /// Number of bytes in the truncated output.
    pub output_bytes: usize,
    /// Whether the last line was partially truncated (tail truncation edge case).
    pub last_line_partial: bool,
    /// Whether the first line exceeded the byte limit (head truncation).
    pub first_line_exceeds_limit: bool,
    /// The max lines limit that was applied.
    pub max_lines: usize,
    /// The max bytes limit that was applied.
    pub max_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncatedBy {
    Lines,
    Bytes,
}

/// Options for truncation helpers.
#[derive(Debug, Clone, Copy, Default)]
pub struct TruncationOptions {
    pub max_lines: Option<usize>,
    pub max_bytes: Option<usize>,
}

fn utf8_byte_length(content: &str) -> usize {
    content.len()
}

fn char_code(ch: char) -> u32 {
    ch as u32
}

fn is_high_surrogate(ch: char) -> bool {
    (0xD800..=0xDBFF).contains(&char_code(ch))
}

fn is_low_surrogate(ch: char) -> bool {
    (0xDC00..=0xDFFF).contains(&char_code(ch))
}

fn replace_unpaired_surrogates(content: &str) -> String {
    let mut output = String::new();
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        if is_high_surrogate(ch) {
            if let Some(&next) = chars.peek()
                && is_low_surrogate(next)
            {
                output.push(ch);
                output.push(chars.next().unwrap());
                continue;
            }
            output.push('\u{FFFD}');
        } else if is_low_surrogate(ch) {
            output.push('\u{FFFD}');
        } else {
            output.push(ch);
        }
    }
    output
}

/// Format bytes as a human-readable size.
pub fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Truncate content from the head (keep first N lines/bytes).
pub fn truncate_head(content: &str, options: TruncationOptions) -> TruncationResult {
    let max_lines = options.max_lines.unwrap_or(DEFAULT_MAX_LINES);
    let max_bytes = options.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);

    let total_bytes = utf8_byte_length(content);
    let lines: Vec<&str> = content.split('\n').collect();
    let total_lines = lines.len();

    if total_lines <= max_lines && total_bytes <= max_bytes {
        return TruncationResult {
            content: content.to_string(),
            truncated: false,
            truncated_by: None,
            total_lines,
            total_bytes,
            output_lines: total_lines,
            output_bytes: total_bytes,
            last_line_partial: false,
            first_line_exceeds_limit: false,
            max_lines,
            max_bytes,
        };
    }

    let first_line_bytes = utf8_byte_length(lines[0]);
    if first_line_bytes > max_bytes {
        return TruncationResult {
            content: String::new(),
            truncated: true,
            truncated_by: Some(TruncatedBy::Bytes),
            total_lines,
            total_bytes,
            output_lines: 0,
            output_bytes: 0,
            last_line_partial: false,
            first_line_exceeds_limit: true,
            max_lines,
            max_bytes,
        };
    }

    let mut output_lines_arr: Vec<&str> = Vec::new();
    let mut output_bytes_count = 0usize;
    let mut truncated_by = TruncatedBy::Lines;

    for (i, line) in lines.iter().enumerate().take(max_lines) {
        let line_bytes = utf8_byte_length(line) + if i > 0 { 1 } else { 0 };

        if output_bytes_count + line_bytes > max_bytes {
            truncated_by = TruncatedBy::Bytes;
            break;
        }

        output_lines_arr.push(line);
        output_bytes_count += line_bytes;
    }

    if output_lines_arr.len() >= max_lines && output_bytes_count <= max_bytes {
        truncated_by = TruncatedBy::Lines;
    }

    let output_content = output_lines_arr.join("\n");
    let final_output_bytes = utf8_byte_length(&output_content);

    TruncationResult {
        content: output_content,
        truncated: true,
        truncated_by: Some(truncated_by),
        total_lines,
        total_bytes,
        output_lines: output_lines_arr.len(),
        output_bytes: final_output_bytes,
        last_line_partial: false,
        first_line_exceeds_limit: false,
        max_lines,
        max_bytes,
    }
}

/// Truncate content from the tail (keep last N lines/bytes).
pub fn truncate_tail(content: &str, options: TruncationOptions) -> TruncationResult {
    let max_lines = options.max_lines.unwrap_or(DEFAULT_MAX_LINES);
    let max_bytes = options.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);

    let total_bytes = utf8_byte_length(content);
    let mut lines: Vec<&str> = content.split('\n').collect();
    if lines.len() > 1 && lines.last() == Some(&"") {
        lines.pop();
    }
    let total_lines = lines.len();

    if total_lines <= max_lines && total_bytes <= max_bytes {
        return TruncationResult {
            content: content.to_string(),
            truncated: false,
            truncated_by: None,
            total_lines,
            total_bytes,
            output_lines: total_lines,
            output_bytes: total_bytes,
            last_line_partial: false,
            first_line_exceeds_limit: false,
            max_lines,
            max_bytes,
        };
    }

    let mut output_lines_arr: Vec<String> = Vec::new();
    let mut output_bytes_count = 0usize;
    let mut truncated_by = TruncatedBy::Lines;
    let mut last_line_partial = false;

    let mut i = lines.len();
    while i > 0 && output_lines_arr.len() < max_lines {
        i -= 1;
        let line = lines[i];
        let line_bytes = utf8_byte_length(line) + if !output_lines_arr.is_empty() { 1 } else { 0 };

        if output_bytes_count + line_bytes > max_bytes {
            truncated_by = TruncatedBy::Bytes;
            if output_lines_arr.is_empty() {
                let truncated_line = truncate_string_to_bytes_from_end(line, max_bytes);
                output_bytes_count = utf8_byte_length(&truncated_line);
                output_lines_arr.insert(0, truncated_line);
                last_line_partial = true;
            }
            break;
        }

        output_lines_arr.insert(0, line.to_string());
        output_bytes_count += line_bytes;
    }

    if output_lines_arr.len() >= max_lines && output_bytes_count <= max_bytes {
        truncated_by = TruncatedBy::Lines;
    }

    let output_content = output_lines_arr.join("\n");
    let final_output_bytes = utf8_byte_length(&output_content);

    TruncationResult {
        content: output_content,
        truncated: true,
        truncated_by: Some(truncated_by),
        total_lines,
        total_bytes,
        output_lines: output_lines_arr.len(),
        output_bytes: final_output_bytes,
        last_line_partial,
        first_line_exceeds_limit: false,
        max_lines,
        max_bytes,
    }
}

fn truncate_string_to_bytes_from_end(str: &str, max_bytes: usize) -> String {
    if max_bytes == 0 {
        return String::new();
    }

    let mut output_bytes = 0usize;
    let mut start = str.len();
    let mut needs_replacement = false;
    let mut i = str.len();

    while i > 0 {
        let character_start = str[..i].char_indices().last().map(|(idx, _)| idx).unwrap_or(0);
        let ch = str[character_start..i].chars().next().unwrap();
        let character_bytes = ch.len_utf8();
        let unpaired_surrogate = is_high_surrogate(ch) || is_low_surrogate(ch);

        if output_bytes + character_bytes > max_bytes {
            break;
        }
        output_bytes += character_bytes;
        start = character_start;
        needs_replacement |= unpaired_surrogate;
        i = character_start;
    }

    let output = &str[start..];
    if needs_replacement {
        replace_unpaired_surrogates(output)
    } else {
        output.to_string()
    }
}

/// Truncate a single line to max characters, adding `[truncated]` suffix.
pub fn truncate_line(line: &str, max_chars: usize) -> (String, bool) {
    if line.chars().count() <= max_chars {
        return (line.to_string(), false);
    }
    let truncated: String = line.chars().take(max_chars).collect();
    (format!("{truncated}... [truncated]"), true)
}
