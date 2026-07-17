//! Shared truncation utilities for tool outputs — elph-agent module.
//!
//! Truncation is based on two independent limits — whichever is hit first wins:
//! - Line limit (default: 2000 lines)
//! - Byte limit (default: 50KB)
//!
//! Never returns partial lines (except bash tail truncation edge case).

use crate::utils::lines::SplitLines;
use crate::utils::lines::{count_lines, line_starts};

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
    /// Total number of lines in the original content.
    pub total_lines: usize,
    /// Total number of bytes in the original content.
    pub total_bytes: usize,
    /// Number of complete lines in the truncated output.
    pub output_lines: usize,
    /// Number of bytes in the truncated output.
    pub output_bytes: usize,
    /// The max lines limit that was applied.
    pub max_lines: usize,
    /// The max bytes limit that was applied.
    pub max_bytes: usize,
    /// Whether truncation occurred.
    pub truncated: bool,
    /// Which limit was hit: `"lines"`, `"bytes"`, or `None` if not truncated.
    pub truncated_by: Option<TruncatedBy>,
    /// Whether the last line was partially truncated (tail truncation edge case).
    pub last_line_partial: bool,
    /// Whether the first line exceeded the byte limit (head truncation).
    pub first_line_exceeds_limit: bool,
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
    let total_lines = count_lines(content);

    if total_lines <= max_lines && total_bytes <= max_bytes {
        return TruncationResult {
            content: content.to_string(),
            total_lines,
            total_bytes,
            output_lines: total_lines,
            output_bytes: total_bytes,
            max_lines,
            max_bytes,
            truncated: false,
            truncated_by: None,
            last_line_partial: false,
            first_line_exceeds_limit: false,
        };
    }

    let mut output_lines = 0usize;
    let mut output_bytes = 0usize;
    let mut included_end = 0usize;
    let mut truncated_by = TruncatedBy::Lines;
    let mut pos = 0usize;

    for line in SplitLines::new(content) {
        let sep_bytes = usize::from(output_lines > 0);
        let line_bytes = line.len() + sep_bytes;

        if output_lines == 0 && line.len() > max_bytes {
            return TruncationResult {
                content: String::new(),
                total_lines,
                total_bytes,
                output_lines: 0,
                output_bytes: 0,
                max_lines,
                max_bytes,
                truncated: true,
                truncated_by: Some(TruncatedBy::Bytes),
                last_line_partial: false,
                first_line_exceeds_limit: true,
            };
        }

        if output_lines >= max_lines {
            truncated_by = TruncatedBy::Lines;
            break;
        }

        if output_bytes + line_bytes > max_bytes {
            truncated_by = TruncatedBy::Bytes;
            break;
        }

        output_bytes += line_bytes;
        output_lines += 1;
        included_end = pos + line.len();
        pos += line.len() + 1;
    }

    if output_lines >= max_lines && output_bytes <= max_bytes {
        truncated_by = TruncatedBy::Lines;
    }

    let output_content = content[..included_end].to_string();
    let final_output_bytes = utf8_byte_length(&output_content);

    TruncationResult {
        content: output_content,
        total_lines,
        total_bytes,
        output_lines,
        output_bytes: final_output_bytes,
        max_lines,
        max_bytes,
        truncated: true,
        truncated_by: Some(truncated_by),
        last_line_partial: false,
        first_line_exceeds_limit: false,
    }
}

/// Truncate content from the tail (keep last N lines/bytes).
pub fn truncate_tail(content: &str, options: TruncationOptions) -> TruncationResult {
    let max_lines = options.max_lines.unwrap_or(DEFAULT_MAX_LINES);
    let max_bytes = options.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);

    let total_bytes = utf8_byte_length(content);
    let mut starts = line_starts(content);
    if starts.len() > 1 && content.ends_with('\n') {
        starts.pop();
    }
    let total_lines = starts.len();

    if total_lines <= max_lines && total_bytes <= max_bytes {
        return TruncationResult {
            content: content.to_string(),
            total_lines,
            total_bytes,
            output_lines: total_lines,
            output_bytes: total_bytes,
            max_lines,
            max_bytes,
            truncated: false,
            truncated_by: None,
            last_line_partial: false,
            first_line_exceeds_limit: false,
        };
    }

    let mut output_lines_arr: Vec<String> = Vec::new();
    let mut output_bytes_count = 0usize;
    let mut truncated_by = TruncatedBy::Lines;
    let mut last_line_partial = false;

    let mut i = starts.len();
    while i > 0 && output_lines_arr.len() < max_lines {
        i -= 1;
        let start = starts[i];
        let line = if let Some(&next_start) = starts.get(i + 1) {
            &content[start..next_start.saturating_sub(1)]
        } else {
            &content[start..]
        };
        let line_bytes = utf8_byte_length(line) + usize::from(!output_lines_arr.is_empty());

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
        total_lines,
        total_bytes,
        output_lines: output_lines_arr.len(),
        output_bytes: final_output_bytes,
        max_lines,
        max_bytes,
        truncated: true,
        truncated_by: Some(truncated_by),
        last_line_partial,
        first_line_exceeds_limit: false,
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

/// Select a line range without allocating intermediate line vectors.
pub fn select_line_range(content: &str, start_line: usize, limit: Option<usize>) -> Result<String, usize> {
    let total_lines = count_lines(content);
    if start_line >= total_lines {
        return Err(total_lines);
    }

    let mut selected = String::new();
    let mut lines_taken = 0usize;

    for (line_idx, line) in SplitLines::new(content).enumerate() {
        if line_idx < start_line {
            continue;
        }
        if let Some(limit) = limit
            && lines_taken >= limit
        {
            break;
        }
        if lines_taken > 0 {
            selected.push('\n');
        }
        selected.push_str(line);
        lines_taken += 1;
    }

    Ok(selected)
}
