use crate::utils::{str_display_width, truncate_to_width_no_ellipsis};

use super::component::Line;
use super::cursor::{LINE_RESET, extract_and_strip_cursor};
use super::terminal::Terminal;

pub const SYNC_BEGIN: &str = "\x1b[?2026h";
pub const SYNC_END: &str = "\x1b[?2026l";

/// State tracked between differential paint passes.
#[derive(Debug, Clone)]
pub struct RenderState {
    pub previous_lines: Vec<Line>,
    pub previous_width: u16,
    pub previous_height: u16,
    pub cursor_row: usize,
    pub max_lines_rendered: usize,
    pub full_redraw_count: u32,
    pub clear_on_shrink: bool,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            previous_lines: Vec::new(),
            previous_width: 0,
            previous_height: 0,
            cursor_row: 0,
            max_lines_rendered: 0,
            full_redraw_count: 0,
            clear_on_shrink: std::env::var("ELPH_CLEAR_ON_SHRINK").ok().is_some_and(|v| v == "1"),
        }
    }
}

impl RenderState {
    pub fn reset(&mut self) {
        self.previous_lines.clear();
        self.previous_width = u16::MAX;
        self.previous_height = u16::MAX;
        self.cursor_row = 0;
        self.max_lines_rendered = 0;
    }

    pub fn set_clear_on_shrink(&mut self, enabled: bool) {
        self.clear_on_shrink = enabled;
    }
}

/// Returns the first line index where `previous` and `next` differ.
pub fn first_changed_line(previous: &[Line], next: &[Line]) -> Option<usize> {
    let common = previous.len().min(next.len());
    for i in 0..common {
        if previous[i] != next[i] {
            return Some(i);
        }
    }
    if previous.len() != next.len() {
        Some(common)
    } else {
        None
    }
}

fn write_lines(terminal: &mut dyn Terminal, lines: &[String]) {
    let mut buf = String::new();
    for line in lines {
        buf.push_str(line);
        buf.push_str("\r\n");
    }
    terminal.write(&buf);
}

fn prepare_line(line: &str, width: u16) -> String {
    let width = width.max(1) as usize;
    let mut out = if str_display_width(line) > width {
        truncate_to_width_no_ellipsis(line, width)
    } else {
        line.to_string()
    };
    out.push_str(LINE_RESET);
    out
}

/// Paint `next_lines` to `terminal`.
pub fn do_render(terminal: &mut dyn Terminal, state: &mut RenderState, next_lines: &[Line]) {
    let width = terminal.columns();
    let height = terminal.rows();
    let width_changed = state.previous_width != 0 && state.previous_width != width;
    let height_changed = state.previous_height != 0 && state.previous_height != height;
    let first_render = state.previous_lines.is_empty();
    let shrunk = next_lines.len() < state.previous_lines.len();
    let shrink_needs_clear = shrunk && state.clear_on_shrink && next_lines.len() < state.max_lines_rendered;

    let mut stripped: Vec<String> = next_lines.to_vec();
    let hardware_cursor = extract_and_strip_cursor(&mut stripped);
    let prepared: Vec<String> = stripped.iter().map(|l| prepare_line(l, width)).collect();

    let needs_full_redraw = first_render
        || width_changed
        || height_changed
        || shrink_needs_clear
        || (shrunk && next_lines.len() + 3 < state.previous_viewport_top());

    terminal.hide_cursor();
    terminal.write(SYNC_BEGIN);

    if needs_full_redraw {
        state.full_redraw_count = state.full_redraw_count.saturating_add(1);
        if !first_render {
            terminal.clear_screen();
            terminal.move_home();
        }
        write_lines(terminal, &prepared);
        state.cursor_row = prepared.len().saturating_sub(1);
        state.max_lines_rendered = state.max_lines_rendered.max(prepared.len());
    } else if let Some(start) = first_changed_line(&state.previous_lines, next_lines) {
        let row_diff = start as i32 - state.cursor_row as i32;
        if row_diff != 0 {
            terminal.move_by(row_diff);
        }
        state.cursor_row = start;
        terminal.clear_from_cursor();
        write_lines(terminal, &prepared[start..]);
        state.cursor_row = prepared.len().saturating_sub(1);
        state.max_lines_rendered = state.max_lines_rendered.max(prepared.len());
    } else if prepared.len() > state.previous_lines.len() {
        // Append-only growth at bottom.
        write_lines(terminal, &prepared[state.previous_lines.len()..]);
        state.cursor_row = prepared.len().saturating_sub(1);
        state.max_lines_rendered = state.max_lines_rendered.max(prepared.len());
    }

    terminal.write(SYNC_END);

    super::hardware_cursor::apply_hardware_cursor(terminal, hardware_cursor);

    state.previous_lines = stripped;
    state.previous_width = width;
    state.previous_height = height;
}

impl RenderState {
    fn previous_viewport_top(&self) -> usize {
        self.previous_lines.len().saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_first_changed_line() {
        let prev = vec!["a".into(), "b".into(), "c".into()];
        let next = vec!["a".into(), "B".into(), "c".into()];
        assert_eq!(first_changed_line(&prev, &next), Some(1));
    }

    #[test]
    fn detects_length_change() {
        let prev = vec!["a".into()];
        let next = vec!["a".into(), "b".into()];
        assert_eq!(first_changed_line(&prev, &next), Some(1));
    }
}
