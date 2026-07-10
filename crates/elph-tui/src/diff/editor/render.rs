use crate::utils::{pad_lines, truncate_to_width_no_ellipsis};

use crate::diff::ansi::RESET as ANSI_RESET;
use crate::diff::ansi::{self, styled};
use crate::diff::component::Line;
use crate::diff::cursor::CURSOR_MARKER;
use crate::diff::text_buffer::{PromptBuffer, char_display_width, expand_for_display};

use super::Editor;

const REVERSE_VIDEO: &str = "\x1b[7m";

impl Editor {
    pub(super) fn content_width(&self, width: u16) -> usize {
        width.saturating_sub(self.padding_x.saturating_mul(2)).max(1) as usize
    }

    pub(super) fn layout(&self, width: u16) -> PromptBuffer {
        PromptBuffer::new(&self.text, self.content_width(width))
    }

    fn ensure_cursor_visible(&mut self, width: u16) {
        let buffer = self.layout(width);
        let (row, _) = buffer.row_column_for_offset(self.cursor);
        let row = row as usize;
        if row < self.scroll_row {
            self.scroll_row = row;
        } else if row >= self.scroll_row + self.max_visible_rows {
            self.scroll_row = row + 1 - self.max_visible_rows;
        }
    }

    fn render_content_line(&self, buffer: &PromptBuffer, row_idx: usize, width: usize) -> String {
        let rows = buffer.rows();
        if row_idx >= rows.len() {
            return String::new();
        }
        let row = &rows[row_idx];
        let slice = &self.text[row.offset..row.offset + row.len];
        let display = expand_for_display(slice);
        let (cursor_row, cursor_col) = buffer.row_column_for_offset(self.cursor);
        let is_cursor_row = row_idx == cursor_row as usize;

        if !is_cursor_row || !self.focused {
            return truncate_to_width_no_ellipsis(&styled(&ansi::fg(self.theme.text), &display), width);
        }

        let col = cursor_col as usize;
        let before = truncate_to_width_no_ellipsis(
            &styled(&ansi::fg(self.theme.text), &slice_at_display_col(&display, 0, col)),
            width,
        );
        let after_start = slice_at_display_col_offset(slice, col);
        let cursor_char = self
            .text
            .get(after_start..)
            .and_then(|s| s.chars().next())
            .unwrap_or(' ');
        let cursor_cell = format!("{CURSOR_MARKER}{REVERSE_VIDEO}{cursor_char}{ANSI_RESET}");
        let after_text = self.text.get(after_start..).unwrap_or("");
        let after_char_len = cursor_char.len_utf8();
        let after = &after_text[after_char_len.min(after_text.len())..];
        let after_display = expand_for_display(after);
        let after_styled = styled(&ansi::fg(self.theme.text), &after_display);
        truncate_to_width_no_ellipsis(&format!("{before}{cursor_cell}{after_styled}"), width)
    }

    pub(super) fn build_lines(&mut self, width: u16) -> Vec<Line> {
        self.ensure_cursor_visible(width);
        let buffer = self.layout(width);
        let rows = buffer.rows();
        let content_width = self.content_width(width);
        let end = (self.scroll_row + self.max_visible_rows).min(rows.len());
        let mut lines = Vec::new();

        if self.scroll_row > 0 {
            lines.push(styled(
                &ansi::fg(self.theme.border),
                &format!("─── ↑ {} more ───", self.scroll_row),
            ));
        }

        for row_idx in self.scroll_row..end {
            lines.push(self.render_content_line(&buffer, row_idx, content_width));
        }

        let remaining = rows.len().saturating_sub(end);
        if remaining > 0 {
            lines.push(styled(
                &ansi::fg(self.theme.border),
                &format!("─── ↓ {remaining} more ───"),
            ));
        }

        pad_lines(&lines, self.padding_x as usize, 0)
    }
}

fn slice_at_display_col(text: &str, start_col: usize, end_col: usize) -> String {
    if end_col <= start_col {
        return String::new();
    }
    let mut col = 0usize;
    let mut out = String::new();
    for ch in text.chars() {
        if col >= end_col {
            break;
        }
        if col >= start_col {
            out.push(ch);
        }
        col += char_display_width(ch, col);
    }
    out
}

fn slice_at_display_col_offset(text: &str, col: usize) -> usize {
    let mut width = 0usize;
    for (idx, ch) in text.char_indices() {
        if width >= col {
            return idx;
        }
        width += char_display_width(ch, width);
    }
    text.len()
}
