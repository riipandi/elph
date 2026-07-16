//! Block segmentation and consistent inter-block spacing.

use super::model::{MarkdownLine, MarkdownLineKind};

pub const CODE_HORIZONTAL_PADDING: u16 = 2;
pub const CODE_VERTICAL_PADDING: u16 = 2;

pub fn code_content_width(outer_width: u16) -> u16 {
    outer_width.saturating_sub(CODE_HORIZONTAL_PADDING).max(1)
}

/// End index (exclusive) for the block segment starting at `start`.
pub fn segment_end(lines: &[MarkdownLine], start: usize) -> usize {
    let Some(line) = lines.get(start) else {
        return start;
    };
    if line.is_blank() {
        return start + 1;
    }
    if line.code_background || line.kind == MarkdownLineKind::Code {
        let mut index = start + 1;
        while index < lines.len()
            && (lines[index].code_background || lines[index].kind == MarkdownLineKind::Code)
            && !lines[index].is_blank()
        {
            index += 1;
        }
        return index;
    }
    if line.kind == MarkdownLineKind::ListItem {
        let mut index = start + 1;
        while index < lines.len() && lines[index].kind == MarkdownLineKind::ListItem && !lines[index].is_blank() {
            index += 1;
        }
        return index;
    }
    start + 1
}

/// Rows of breathing room after the segment at `start` (0 when last segment).
pub fn segment_gap_after(lines: &[MarkdownLine], start: usize, end: usize) -> u16 {
    if end >= lines.len() {
        return 0;
    }
    let line = &lines[start];
    if line.is_blank() {
        return 0;
    }
    if line.code_background || line.kind == MarkdownLineKind::Code {
        return 1;
    }
    block_gap_after(lines, end.saturating_sub(1))
}

/// Gap after one line, based on the next line (single source of truth for spacing).
pub fn block_gap_after(lines: &[MarkdownLine], index: usize) -> u16 {
    if index + 1 >= lines.len() {
        return 0;
    }
    let line = &lines[index];
    let next = &lines[index + 1];
    if line.is_blank() || next.is_blank() {
        return 0;
    }
    match line.kind {
        MarkdownLineKind::Continuation => 0,
        MarkdownLineKind::ListItem => {
            if next.kind == MarkdownLineKind::ListItem {
                0
            } else {
                1
            }
        }
        MarkdownLineKind::Code => {
            if next.code_background || next.kind == MarkdownLineKind::Code {
                0
            } else {
                1
            }
        }
        _ => {
            if next.kind == MarkdownLineKind::Continuation {
                0
            } else {
                1
            }
        }
    }
}
