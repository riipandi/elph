//! Wrapped row layout approximating iocraft multiline [`TextInput`].

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Wrap width inside iocraft [`TextInput`] (reserves one column for the cursor).
pub fn text_input_wrap_width(viewport_width: u16) -> usize {
    viewport_width.max(1).saturating_sub(1) as usize
}

/// Wrap width for [`Text`] with an overlay cursor (no reserved column).
pub fn overlay_editor_wrap_width(viewport_width: u16) -> usize {
    viewport_width.max(1) as usize
}

#[derive(Debug, Clone)]
pub struct TextRow {
    pub offset: usize,
    pub len: usize,
    pub width: usize,
}

/// Wrapped row index for scroll metrics and cursor tracking (does not own the source text).
#[derive(Debug, Clone)]
pub struct WrappedTextLayout {
    pub(crate) rows: Vec<TextRow>,
}

impl WrappedTextLayout {
    pub fn new(text: &str, viewport_width: u16) -> Self {
        Self::with_max_width(text, text_input_wrap_width(viewport_width))
    }

    /// Wrapped rows aligned with iocraft [`Text`] (`TextWrap::Wrap`) in a fixed-width column.
    pub fn new_for_overlay_editor(text: &str, viewport_width: u16) -> Self {
        Self::with_max_width(text, overlay_editor_wrap_width(viewport_width))
    }

    fn with_max_width(text: &str, max_width: usize) -> Self {
        let mut rows = Vec::new();

        if text.is_empty() {
            rows.push(TextRow {
                offset: 0,
                len: 0,
                width: 0,
            });
            return Self { rows };
        }

        let mut line_start = 0usize;
        for (newline_idx, _) in text.match_indices('\n') {
            Self::push_wrapped_line(text, line_start, newline_idx, max_width, &mut rows);
            line_start = newline_idx + 1;
        }
        Self::push_wrapped_line(text, line_start, text.len(), max_width, &mut rows);

        if rows.is_empty() {
            rows.push(TextRow {
                offset: 0,
                len: 0,
                width: 0,
            });
        }

        Self { rows }
    }

    /// Incremental update when `new_text` extends `old_text` by a small suffix (typing at EOF).
    pub fn try_extend_suffix(prev: &Self, old_text: &str, new_text: &str, max_width: usize) -> Option<Self> {
        if new_text.len() < old_text.len() || !new_text.starts_with(old_text) {
            return None;
        }
        let suffix = &new_text[old_text.len()..];
        if suffix.is_empty() || suffix.len() > 8 {
            return None;
        }
        Some(prev.rewrap_tail(old_text.len(), new_text, max_width))
    }

    /// Incremental update when `new_text` is a small backspace/delete from `old_text` at EOF.
    pub fn try_truncate_suffix(prev: &Self, old_text: &str, new_text: &str, max_width: usize) -> Option<Self> {
        if new_text.len() >= old_text.len() || !old_text.starts_with(new_text) {
            return None;
        }
        let removed = old_text.len() - new_text.len();
        if removed == 0 || removed > 8 {
            return None;
        }
        Some(prev.rewrap_tail(new_text.len(), new_text, max_width))
    }

    fn rewrap_tail(&self, tail_anchor: usize, text: &str, max_width: usize) -> Self {
        let line_start = text[..tail_anchor.min(text.len())]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let truncate_from = self
            .rows
            .iter()
            .position(|row| row.offset >= line_start)
            .unwrap_or(self.rows.len());
        let mut rows: Vec<TextRow> = self.rows[..truncate_from].to_vec();
        let mut segment_start = line_start;
        for (newline_idx, _) in text[line_start..].match_indices('\n') {
            let newline_idx = line_start + newline_idx;
            Self::push_wrapped_line(text, segment_start, newline_idx, max_width, &mut rows);
            segment_start = newline_idx + 1;
        }
        Self::push_wrapped_line(text, segment_start, text.len(), max_width, &mut rows);
        if rows.is_empty() {
            rows.push(TextRow {
                offset: 0,
                len: 0,
                width: 0,
            });
        }
        Self { rows }
    }

    fn push_wrapped_line(text: &str, start: usize, end: usize, max_width: usize, rows: &mut Vec<TextRow>) {
        let slice = &text[start..end];
        if slice.is_empty() {
            rows.push(TextRow {
                offset: start,
                len: 0,
                width: 0,
            });
            return;
        }

        let mut row_start = 0usize;
        let mut col = 0usize;
        for (idx, ch) in slice.char_indices() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if col > 0 && col + w > max_width {
                rows.push(TextRow {
                    offset: start + row_start,
                    len: idx - row_start,
                    width: col,
                });
                row_start = idx;
                col = w;
            } else {
                col += w;
            }
        }
        let tail = &slice[row_start..];
        rows.push(TextRow {
            offset: start + row_start,
            len: tail.len(),
            width: col,
        });
    }

    pub fn row_count(&self) -> u16 {
        self.rows.len().max(1) as u16
    }

    /// Wrapped display lines for the full source text.
    pub fn wrapped_line_strings(&self, text: &str) -> Vec<String> {
        if self.rows.is_empty() {
            return vec![String::new()];
        }
        self.rows
            .iter()
            .map(|row| text[row.offset..row.offset + row.len].to_string())
            .collect()
    }

    fn row_index_for_offset(&self, offset: usize) -> usize {
        let mut lo = 0usize;
        let mut hi = self.rows.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.rows[mid].offset <= offset {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo.saturating_sub(1)
    }

    pub fn row_column_for_offset(&self, text: &str, offset: usize) -> (u16, u16) {
        let offset = offset.min(text.len());
        let row_idx = self.row_index_for_offset(offset);
        let row = &self.rows[row_idx];
        let offset_in_row = offset.saturating_sub(row.offset);
        if offset_in_row <= row.len {
            let col = display_width(&text[row.offset..offset]) as u16;
            return (row_idx as u16, col);
        }
        (
            self.rows.len().saturating_sub(1) as u16,
            self.rows.last().map_or(0, |r| r.width as u16),
        )
    }

    pub fn left_of_offset(text: &str, offset: usize) -> usize {
        if offset == 0 {
            0
        } else {
            text[..offset].char_indices().last().map_or(0, |(i, _)| i)
        }
    }

    pub fn right_of_offset(text: &str, offset: usize) -> usize {
        if offset >= text.len() {
            text.len()
        } else {
            text[offset..]
                .char_indices()
                .nth(1)
                .map_or(text.len(), |(i, _)| offset + i)
        }
    }

    fn offset_for_closest_column_in_row(&self, text: &str, row: u16, col: u16) -> usize {
        if self.rows.is_empty() {
            return 0;
        }
        let row_idx = (row as usize).min(self.rows.len() - 1);
        let row = &self.rows[row_idx];
        let col = col as usize;
        if col >= row.width {
            return row.offset + row.len;
        }
        let mut width = 0;
        for (idx, c) in text[row.offset..].char_indices() {
            if width >= col {
                return row.offset + idx;
            }
            width += UnicodeWidthChar::width(c).unwrap_or(0);
        }
        row.offset + row.len
    }

    /// Byte offset for a display row/column (mouse hit-testing).
    pub fn offset_at_row_col(&self, text: &str, row: u16, col: u16) -> usize {
        self.offset_for_closest_column_in_row(text, row, col)
    }

    pub fn above_offset(&self, text: &str, offset: usize, col_preference: Option<u16>) -> usize {
        let (row, col) = self.row_column_for_offset(text, offset);
        if row == 0 {
            return offset;
        }
        self.offset_for_closest_column_in_row(text, row - 1, col_preference.unwrap_or(col))
    }

    pub fn below_offset(&self, text: &str, offset: usize, col_preference: Option<u16>) -> usize {
        let (row, col) = self.row_column_for_offset(text, offset);
        if row as usize + 1 >= self.rows.len() {
            return offset;
        }
        self.offset_for_closest_column_in_row(text, row + 1, col_preference.unwrap_or(col))
    }

    pub fn row_start_offset(&self, text: &str, offset: usize) -> usize {
        let row_idx = self.row_index_for_offset(offset.min(text.len()));
        self.rows[row_idx].offset
    }

    pub fn row_end_offset(&self, text: &str, offset: usize) -> usize {
        let row_idx = self.row_index_for_offset(offset.min(text.len()));
        let r = &self.rows[row_idx];
        r.offset + r.len
    }

    /// Pre-wrapped row text for a viewport slice (`TextWrap::NoWrap`, one source row per display line).
    pub fn display_text_for_row_range(&self, text: &str, scroll_row: u16, viewport_rows: u16) -> String {
        if self.rows.is_empty() || viewport_rows == 0 {
            return String::new();
        }
        let start = (scroll_row as usize).min(self.rows.len().saturating_sub(1));
        let end = (start + viewport_rows as usize).min(self.rows.len());
        if start >= end {
            return String::new();
        }
        self.rows[start..end]
            .iter()
            .map(|row| &text[row.offset..row.offset + row.len])
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// First `max_lines` wrapped rows as explicit newlines; ellipsis on the last line when clipped.
    pub fn clamp_display_lines(&self, text: &str, max_lines: u16, max_line_width: usize) -> (String, u16, bool) {
        let max = max_lines.max(1) as usize;
        let truncated = self.rows.len() > max;
        let take = if truncated { max } else { self.rows.len() };
        let mut lines: Vec<String> = Vec::with_capacity(take);
        for (i, row) in self.rows.iter().take(take).enumerate() {
            let segment = &text[row.offset..row.offset + row.len];
            if truncated && i + 1 == take {
                lines.push(mark_clamped_line(segment, max_line_width));
            } else {
                lines.push(segment.to_string());
            }
        }
        let rows = take.min(u16::MAX as usize) as u16;
        (lines.join("\n"), rows, truncated)
    }
}

fn display_width(slice: &str) -> usize {
    slice.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum()
}

fn mark_clamped_line(line: &str, max_line_width: usize) -> String {
    const MARKER: &str = " …";
    let marker_w = MARKER.width();
    if max_line_width == 0 {
        return String::new();
    }
    if marker_w >= max_line_width {
        return "…".to_string();
    }
    let body_budget = max_line_width - marker_w;
    let body = crate::utils::truncate_with_ellipsis(line, body_budget);
    if body.ends_with('…') {
        body
    } else {
        format!("{body}{MARKER}")
    }
}

pub fn update_scroll_offset(current: u16, cursor_row: u16, viewport_height: u16, content_height: u16) -> u16 {
    if viewport_height == 0 {
        return 0;
    }
    let mut offset = current;
    if cursor_row >= offset.saturating_add(viewport_height) {
        offset = cursor_row.saturating_sub(viewport_height.saturating_sub(1));
    } else if cursor_row < offset {
        offset = cursor_row;
    }
    let max_offset = content_height.saturating_sub(viewport_height);
    offset.min(max_offset)
}
