//! Word-wrap helpers for ANSI line emission.

use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

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

fn char_display_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

fn str_display_width(s: &str) -> usize {
    s.chars().map(char_display_width).sum()
}

fn trailing_whitespace_width(segment: &str) -> usize {
    segment
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .map(char_display_width)
        .sum()
}

/// Wrap `text` into display lines at `wrap_width` (UAX #14 line breaks).
pub fn wrap_text_to_lines(text: &str, wrap_width: usize) -> Vec<String> {
    let wrap_width = wrap_width.max(1);
    if text.is_empty() {
        return Vec::new();
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    let mut line_start = 0usize;

    for (break_pos, opportunity) in linebreaks(text) {
        let segment = &text[line_start..break_pos];
        let segment_width = str_display_width(segment);
        let trailing_ws = trailing_whitespace_width(segment);
        line_start = break_pos;

        let fit_width = segment_width.saturating_sub(trailing_ws);
        if current_width + fit_width <= wrap_width {
            current.push_str(segment);
            current_width += segment_width;
        } else {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            let content_end = segment
                .char_indices()
                .rev()
                .take_while(|(_, c)| c.is_whitespace())
                .last()
                .map(|(i, _)| i)
                .unwrap_or(segment.len());
            let content = &segment[..content_end];
            if str_display_width(content) > wrap_width {
                let mut w = 0usize;
                let mut chunk = String::new();
                for c in content.chars() {
                    let cw = char_display_width(c);
                    if w > 0 && w + cw > wrap_width {
                        lines.push(std::mem::take(&mut chunk));
                        w = 0;
                    }
                    chunk.push(c);
                    w += cw;
                }
                current = chunk;
                current_width = w;
            } else {
                current.push_str(segment);
                current_width = segment_width;
            }
        }

        if opportunity == BreakOpportunity::Mandatory {
            lines.push(std::mem::take(&mut current));
            let _ = current_width;
            current_width = 0;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    if text.ends_with('\n') {
        lines.push(String::new());
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Hanging-indent wrap ranges for code lines (char indices).
pub fn wrap_with_hanging_ranges(text: &str, inner: u16) -> Vec<(usize, usize)> {
    let inner = inner.max(1) as usize;
    if text.is_empty() {
        return vec![(0, 0)];
    }
    let chars: Vec<char> = text.chars().collect();
    let char_width = |c: char| UnicodeWidthChar::width(c).unwrap_or(0);
    let indent = chars
        .iter()
        .take_while(|c| **c == ' ')
        .count()
        .min(inner.saturating_sub(1));
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut row_start = 0usize;
    let mut row_width = 0usize;
    let mut last_space: Option<usize> = None;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let w = char_width(c);
        let is_space = c == ' ';
        let avail = if ranges.is_empty() { inner } else { inner - indent };
        if row_width + w > avail && row_width > 0 {
            if let Some(sp) = last_space {
                ranges.push((row_start, sp + 1));
                row_start = sp + 1;
                row_width = 0;
                last_space = None;
                continue;
            }
            ranges.push((row_start, i));
            row_start = i;
            row_width = 0;
            last_space = None;
        }
        row_width += w;
        if is_space {
            last_space = Some(i);
        }
        i += 1;
    }
    ranges.push((row_start, chars.len()));
    ranges
}
