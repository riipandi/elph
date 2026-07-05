use unicode_width::UnicodeWidthChar;

/// Terminal tab width used for layout and rendering.
pub const TAB_STOP: usize = 8;

/// Display width of a character at the given column (tabs advance to the next stop).
pub fn char_display_width(ch: char, col: usize) -> usize {
    match ch {
        '\t' => TAB_STOP - (col % TAB_STOP),
        '\r' => 0,
        ch => ch.width().unwrap_or(0),
    }
}

/// Total display width of a string.
pub fn str_display_width(s: &str) -> usize {
    let mut col = 0usize;
    for ch in s.chars() {
        col += char_display_width(ch, col);
    }
    col
}

/// Expands tabs to spaces so rendered text matches layout calculations.
pub fn expand_for_display(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut col = 0usize;
    for ch in s.chars() {
        match ch {
            '\t' => {
                let spaces = char_display_width('\t', col);
                out.extend(std::iter::repeat_n(' ', spaces));
                col += spaces;
            }
            '\r' => {}
            ch => {
                out.push(ch);
                col += char_display_width(ch, col);
            }
        }
    }
    out
}

/// A wrapped display row backed by byte offsets into the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptBufferRow {
    pub offset: usize,
    pub len: usize,
    pub width: usize,
}

/// Wraps prompt text for display and cursor navigation (mirrors iocraft `TextBuffer` layout).
#[derive(Debug, Clone)]
pub struct PromptBuffer {
    text: String,
    rows: Vec<PromptBufferRow>,
}

impl PromptBuffer {
    pub fn new(text: &str, wrap_width: usize) -> Self {
        let wrap_width = wrap_width.max(1);
        let mut rows = Vec::new();

        if text.is_empty() {
            rows.push(PromptBufferRow {
                offset: 0,
                len: 0,
                width: 0,
            });
            return Self {
                text: text.to_string(),
                rows,
            };
        }

        let mut line_start = 0usize;
        for (idx, ch) in text.char_indices() {
            if ch == '\n' {
                rows.extend(wrap_segment(&text[line_start..idx], line_start, wrap_width));
                line_start = idx + ch.len_utf8();
            }
        }
        rows.extend(wrap_segment(&text[line_start..], line_start, wrap_width));

        if rows.is_empty() {
            rows.push(PromptBufferRow {
                offset: text.len(),
                len: 0,
                width: 0,
            });
        }

        Self {
            text: text.to_string(),
            rows,
        }
    }

    pub fn rows(&self) -> &[PromptBufferRow] {
        &self.rows
    }

    pub fn row_column_for_offset(&self, offset: usize) -> (u16, u16) {
        let offset = offset.min(self.text.len());
        for (i, row) in self.rows.iter().enumerate() {
            if offset >= row.offset {
                let offset_in_row = offset - row.offset;
                if offset_in_row <= row.len {
                    let col = str_display_width(&self.text[row.offset..offset]) as u16;
                    return (i as u16, col);
                }
            }
        }
        (self.rows.len() as u16, self.rows.last().map_or(0, |r| r.width as u16))
    }

    pub fn left_of_offset(&self, offset: usize) -> usize {
        if offset == 0 {
            0
        } else {
            prev_char_index(&self.text, offset)
        }
    }

    pub fn right_of_offset(&self, offset: usize) -> usize {
        if offset >= self.text.len() {
            self.text.len()
        } else {
            self.text[offset..]
                .char_indices()
                .nth(1)
                .map_or(self.text.len(), |(i, _)| offset + i)
        }
    }

    pub fn above_offset(&self, offset: usize, col_preference: Option<u16>) -> usize {
        let (row, col) = self.row_column_for_offset(offset);
        if row == 0 {
            return offset;
        }
        self.offset_for_closest_column_in_row(row - 1, col_preference.unwrap_or(col))
    }

    pub fn below_offset(&self, offset: usize, col_preference: Option<u16>) -> usize {
        let (row, col) = self.row_column_for_offset(offset);
        if row as usize + 1 >= self.rows.len() {
            return offset;
        }
        self.offset_for_closest_column_in_row(row + 1, col_preference.unwrap_or(col))
    }

    pub fn row_start_offset(&self, offset: usize) -> usize {
        let (row, _) = self.row_column_for_offset(offset);
        self.rows[row as usize].offset
    }

    pub fn row_end_offset(&self, offset: usize) -> usize {
        let (row, _) = self.row_column_for_offset(offset);
        let row = &self.rows[row as usize];
        row.offset + row.len
    }

    pub fn offset_for_closest_column_in_row(&self, row: u16, col: u16) -> usize {
        let row = &self.rows[row as usize];
        let col = col as usize;
        if col >= row.width {
            return row.offset + row.len;
        }

        let mut width = 0usize;
        for (idx, ch) in self.text[row.offset..].char_indices() {
            if width >= col {
                return row.offset + idx;
            }
            width += char_display_width(ch, width);
        }
        row.offset + row.len
    }
}

fn wrap_segment(segment: &str, start_offset: usize, wrap_width: usize) -> Vec<PromptBufferRow> {
    if segment.is_empty() {
        return vec![PromptBufferRow {
            offset: start_offset,
            len: 0,
            width: 0,
        }];
    }

    let mut rows = Vec::new();
    let mut row_start = 0usize;
    let mut col = 0usize;

    for (idx, ch) in segment.char_indices() {
        let ch_width = char_display_width(ch, col);
        if col > 0 && col + ch_width > wrap_width {
            let len = idx - row_start;
            rows.push(PromptBufferRow {
                offset: start_offset + row_start,
                len,
                width: str_display_width(&segment[row_start..idx]),
            });
            row_start = idx;
            col = 0;
        }

        let ch_width = char_display_width(ch, col);
        col += ch_width;
    }

    rows.push(PromptBufferRow {
        offset: start_offset + row_start,
        len: segment.len() - row_start,
        width: str_display_width(&segment[row_start..]),
    });
    rows
}

/// Returns the number of wrapped rows a single logical line occupies.
pub fn wrapped_row_count(segment: &str, wrap_width: usize) -> u16 {
    wrap_segment(segment, 0, wrap_width.max(1)).len() as u16
}

fn prev_char_index(text: &str, index: usize) -> usize {
    text[..index].char_indices().last().map_or(0, |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_moves_vertically_across_wrapped_rows() {
        let buffer = PromptBuffer::new("foo\nbar baz", 10);
        assert_eq!(buffer.left_of_offset(2), 1);
        assert_eq!(buffer.right_of_offset(2), 3);
        assert_eq!(buffer.above_offset(2, None), 2);
        assert_eq!(buffer.below_offset(2, None), 6);
        assert_eq!(buffer.below_offset(2, Some(5)), 9);
        assert_eq!(buffer.above_offset(5, None), 1);
    }

    #[test]
    fn row_column_for_wide_characters() {
        assert_eq!(PromptBuffer::new("一二!", 10).row_column_for_offset(7), (0, 5));
    }

    #[test]
    fn trailing_newline_puts_cursor_on_next_row() {
        assert_eq!(PromptBuffer::new("asd\n", 10).row_column_for_offset(4), (1, 0));
    }

    #[test]
    fn tab_advances_to_next_stop() {
        assert_eq!(char_display_width('\t', 0), 8);
        assert_eq!(char_display_width('\t', 3), 5);
        assert_eq!(str_display_width("\t\"name\""), 14);
    }

    #[test]
    fn wraps_tab_indented_json_line() {
        let buffer = PromptBuffer::new("{\n\t\"name\": \"elph\"\n}", 12);
        assert!(buffer.rows().len() >= 3);
        assert!(buffer.rows()[1].width <= 12);
    }

    #[test]
    fn expand_for_display_replaces_tabs_with_spaces() {
        assert_eq!(expand_for_display("\t{a}"), "        {a}");
    }
}
