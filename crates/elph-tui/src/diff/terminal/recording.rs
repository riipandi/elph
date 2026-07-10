use anyhow::Result;

use super::Terminal;

/// In-memory terminal for tests; records writes and maintains a simple viewport.
#[derive(Debug)]
pub struct RecordingTerminal {
    pub writes: String,
    pub columns: u16,
    pub rows: u16,
    viewport: Vec<String>,
    cursor_row: usize,
}

impl RecordingTerminal {
    pub fn new(columns: u16, rows: u16) -> Self {
        Self {
            writes: String::new(),
            columns,
            rows,
            viewport: vec![String::new(); rows as usize],
            cursor_row: 0,
        }
    }

    pub fn clear_writes(&mut self) {
        self.writes.clear();
    }

    pub fn get_writes(&self) -> &str {
        &self.writes
    }

    pub fn viewport(&self) -> Vec<String> {
        self.viewport.clone()
    }

    fn apply_write(&mut self, data: &str) {
        let mut line_buf = String::new();
        let mut last_completed_row = None;
        let mut chars = data.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                if !line_buf.is_empty() {
                    self.flush_line_buf(&mut line_buf);
                }
                self.parse_escape(&mut chars);
                continue;
            }
            match ch {
                '\r' => {}
                '\n' => {
                    self.flush_line_buf(&mut line_buf);
                    last_completed_row = Some(self.cursor_row);
                    if self.cursor_row + 1 < self.viewport.len() {
                        self.cursor_row += 1;
                    }
                }
                c => line_buf.push(c),
            }
        }
        if !line_buf.is_empty() {
            self.flush_line_buf(&mut line_buf);
        }
        // `do_render` splits `line` and `\r\n` into separate writes; anchor on the
        // last finished row so cursor tracking matches `RenderState::cursor_row`.
        if let Some(row) = last_completed_row {
            self.cursor_row = row;
        }
    }

    fn flush_line_buf(&mut self, buf: &mut String) {
        if buf.is_empty() {
            return;
        }
        if self.cursor_row < self.viewport.len() {
            self.viewport[self.cursor_row] = Self::strip_ansi(buf);
        }
        buf.clear();
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut in_escape = false;
        for ch in s.chars() {
            if in_escape {
                if ch.is_ascii_alphabetic() || ch == '\x07' {
                    in_escape = false;
                }
                continue;
            }
            if ch == '\x1b' {
                in_escape = true;
                continue;
            }
            out.push(ch);
        }
        out
    }

    fn parse_escape(&mut self, chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
        let mut seq = String::from("\x1b");
        while let Some(&c) = chars.peek() {
            seq.push(c);
            chars.next();
            if c.is_ascii_alphabetic() || c == '\x07' {
                break;
            }
        }

        if seq == "\x1b[2J" {
            for line in &mut self.viewport {
                line.clear();
            }
            self.cursor_row = 0;
            return;
        }
        if seq == "\x1b[0J" || seq == "\x1b[J" {
            for line in self.viewport.iter_mut().skip(self.cursor_row) {
                line.clear();
            }
            return;
        }
        if seq == "\x1b[2K" {
            if self.cursor_row < self.viewport.len() {
                self.viewport[self.cursor_row].clear();
            }
            return;
        }
        if let Some(rest) = seq.strip_prefix("\x1b[")
            && let Some(num) = rest.strip_suffix('A')
            && let Ok(n) = num.parse::<usize>()
        {
            self.cursor_row = self.cursor_row.saturating_sub(n);
        } else if let Some(rest) = seq.strip_prefix("\x1b[")
            && let Some(num) = rest.strip_suffix('B')
            && let Ok(n) = num.parse::<usize>()
        {
            self.cursor_row = (self.cursor_row + n).min(self.viewport.len().saturating_sub(1));
        }
    }
}

impl Terminal for RecordingTerminal {
    fn start(&mut self, _on_input: Box<dyn FnMut(&str) + Send>, _on_resize: Box<dyn FnMut() + Send>) -> Result<()> {
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn write(&mut self, data: &str) {
        // Strip synchronized output wrappers from viewport simulation.
        let stripped = data
            .replace(crate::diff::render::SYNC_BEGIN, "")
            .replace(crate::diff::render::SYNC_END, "");
        self.writes.push_str(data);
        self.apply_write(&stripped);
    }

    fn columns(&self) -> u16 {
        self.columns
    }

    fn rows(&self) -> u16 {
        self.rows
    }

    fn move_by(&mut self, lines: i32) {
        if lines > 0 {
            self.cursor_row = (self.cursor_row + lines as usize).min(self.viewport.len().saturating_sub(1));
        } else {
            self.cursor_row = self.cursor_row.saturating_sub((-lines) as usize);
        }
    }

    fn move_home(&mut self) {
        self.cursor_row = 0;
    }

    fn move_to(&mut self, col: u16, row: u16) {
        self.cursor_row = row as usize;
        let _ = col;
    }

    fn hide_cursor(&mut self) {}
    fn show_cursor(&mut self) {}
    fn clear_line(&mut self) {
        if self.cursor_row < self.viewport.len() {
            self.viewport[self.cursor_row].clear();
        }
    }
    fn clear_from_cursor(&mut self) {
        for line in self.viewport.iter_mut().skip(self.cursor_row) {
            line.clear();
        }
    }
    fn clear_screen(&mut self) {
        self.writes.push_str("\x1b[2J");
        for line in &mut self.viewport {
            line.clear();
        }
        self.cursor_row = 0;
    }
}
