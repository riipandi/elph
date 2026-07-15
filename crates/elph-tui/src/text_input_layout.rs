//! Wrapped row layout approximating iocraft multiline [`TextInput`].

use unicode_width::UnicodeWidthChar;

/// Wrap width inside iocraft [`TextInput`] (reserves one column for the cursor).
pub fn text_input_wrap_width(viewport_width: u16) -> usize {
    viewport_width.max(1).saturating_sub(1) as usize
}

#[derive(Debug, Clone)]
struct TextRow {
    offset: usize,
    len: usize,
    width: usize,
}

/// Wrapped text layout for scroll metrics and cursor row tracking.
#[derive(Debug, Clone)]
pub struct WrappedTextLayout {
    text: String,
    rows: Vec<TextRow>,
}

impl WrappedTextLayout {
    pub fn new(text: &str, viewport_width: u16) -> Self {
        let text = text.to_string();
        let max_width = text_input_wrap_width(viewport_width);
        let mut rows = Vec::new();

        if text.is_empty() {
            rows.push(TextRow {
                offset: 0,
                len: 0,
                width: 0,
            });
            return Self { text, rows };
        }

        let mut line_start = 0usize;
        for (newline_idx, _) in text.match_indices('\n') {
            Self::push_wrapped_line(&text, line_start, newline_idx, max_width, &mut rows);
            line_start = newline_idx + 1;
        }
        Self::push_wrapped_line(&text, line_start, text.len(), max_width, &mut rows);

        if rows.is_empty() {
            rows.push(TextRow {
                offset: 0,
                len: 0,
                width: 0,
            });
        }

        Self { text, rows }
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
                let len = idx - row_start;
                let width = slice[row_start..idx]
                    .chars()
                    .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
                    .sum();
                rows.push(TextRow {
                    offset: start + row_start,
                    len,
                    width,
                });
                row_start = idx;
                col = 0;
            }
            col += w;
        }
        let tail = &slice[row_start..];
        rows.push(TextRow {
            offset: start + row_start,
            len: tail.len(),
            width: tail.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum(),
        });
    }

    pub fn row_count(&self) -> u16 {
        self.rows.len().max(1) as u16
    }

    pub fn row_column_for_offset(&self, offset: usize) -> (u16, u16) {
        let offset = offset.min(self.text.len());
        for (i, row) in self.rows.iter().enumerate() {
            if offset >= row.offset {
                let offset_in_row = offset - row.offset;
                if offset_in_row <= row.len {
                    let col = self.text[row.offset..offset]
                        .chars()
                        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
                        .sum::<usize>() as u16;
                    return (i as u16, col);
                }
            }
        }
        (
            self.rows.len().saturating_sub(1) as u16,
            self.rows.last().map_or(0, |r| r.width as u16),
        )
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
