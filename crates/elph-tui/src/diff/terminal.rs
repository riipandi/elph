use anyhow::Result;
use std::io::{self, IsTerminal, Write};

use crossterm::{
    cursor, queue,
    terminal::{self, ClearType},
};

/// Terminal backend for differential rendering.
pub trait Terminal {
    fn start(&mut self, on_input: Box<dyn FnMut(&str) + Send>, on_resize: Box<dyn FnMut() + Send>) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn write(&mut self, data: &str);
    fn columns(&self) -> u16;
    fn rows(&self) -> u16;
    fn move_by(&mut self, lines: i32);
    fn move_home(&mut self);
    fn move_to(&mut self, col: u16, row: u16);
    fn hide_cursor(&mut self);
    fn show_cursor(&mut self);
    fn clear_line(&mut self);
    fn clear_from_cursor(&mut self);
    fn clear_screen(&mut self);
}

/// Opens the writer that drives the TUI. Falls back to `/dev/tty` when stdout is captured.
pub fn open_tui_writer() -> Result<Box<dyn Write + Send>> {
    if io::stdout().is_terminal() {
        return Ok(Box::new(io::stdout()));
    }
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        if let Ok(f) = OpenOptions::new().read(true).write(true).open("/dev/tty") {
            return Ok(Box::new(f));
        }
    }
    Ok(Box::new(io::stdout()))
}

/// Crossterm-backed terminal for live sessions.
pub struct CrosstermTerminal {
    writer: Box<dyn Write + Send>,
    columns: u16,
    rows: u16,
    raw_mode: bool,
}

impl CrosstermTerminal {
    pub fn new() -> Result<Self> {
        let (columns, rows) = terminal::size().unwrap_or((80, 24));
        Ok(Self {
            writer: open_tui_writer()?,
            columns,
            rows,
            raw_mode: false,
        })
    }

    pub fn with_size(columns: u16, rows: u16) -> Result<Self> {
        Ok(Self {
            writer: open_tui_writer()?,
            columns,
            rows,
            raw_mode: false,
        })
    }

    fn flush(&mut self) {
        let _ = self.writer.flush();
    }

    fn queue_command<I>(&mut self, cmd: I) -> Result<()>
    where
        I: crossterm::Command,
    {
        queue!(self.writer, cmd)?;
        self.flush();
        Ok(())
    }
}

impl Terminal for CrosstermTerminal {
    fn start(&mut self, _on_input: Box<dyn FnMut(&str) + Send>, _on_resize: Box<dyn FnMut() + Send>) -> Result<()> {
        if !self.raw_mode {
            terminal::enable_raw_mode()?;
            self.raw_mode = true;
        }
        self.hide_cursor();
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.show_cursor();
        if self.raw_mode {
            terminal::disable_raw_mode()?;
            self.raw_mode = false;
        }
        Ok(())
    }

    fn write(&mut self, data: &str) {
        let _ = self.writer.write_all(data.as_bytes());
        self.flush();
    }

    fn columns(&self) -> u16 {
        self.columns
    }

    fn rows(&self) -> u16 {
        self.rows
    }

    fn move_by(&mut self, lines: i32) {
        if lines == 0 {
            return;
        }
        if lines > 0 {
            let _ = self.queue_command(cursor::MoveDown(lines as u16));
        } else {
            let _ = self.queue_command(cursor::MoveUp((-lines) as u16));
        }
    }

    fn move_home(&mut self) {
        let _ = self.queue_command(cursor::MoveTo(0, 0));
    }

    fn move_to(&mut self, col: u16, row: u16) {
        let _ = self.queue_command(cursor::MoveTo(col, row));
    }

    fn hide_cursor(&mut self) {
        let _ = self.queue_command(cursor::Hide);
    }

    fn show_cursor(&mut self) {
        let _ = self.queue_command(cursor::Show);
    }

    fn clear_line(&mut self) {
        let _ = self.queue_command(terminal::Clear(ClearType::CurrentLine));
    }

    fn clear_from_cursor(&mut self) {
        let _ = self.queue_command(terminal::Clear(ClearType::FromCursorDown));
    }

    fn clear_screen(&mut self) {
        let _ = self.queue_command(terminal::Clear(ClearType::All));
    }
}

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
        if let Some(rest) = seq.strip_prefix("\x1b[") {
            if let Some(num) = rest.strip_suffix('A') {
                if let Ok(n) = num.parse::<usize>() {
                    self.cursor_row = self.cursor_row.saturating_sub(n);
                }
            } else if let Some(num) = rest.strip_suffix('B')
                && let Ok(n) = num.parse::<usize>()
            {
                self.cursor_row = (self.cursor_row + n).min(self.viewport.len().saturating_sub(1));
            }
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
            .replace(super::render::SYNC_BEGIN, "")
            .replace(super::render::SYNC_END, "");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::component::LineComponent;
    use crate::diff::component::TextBlock;
    use crate::diff::render::{RenderState, do_render};

    #[test]
    fn recording_terminal_tracks_spinner_updates() {
        let mut terminal = RecordingTerminal::new(40, 10);
        let mut block = TextBlock::new(["Header", "Working...", "Footer"]);
        let mut state = RenderState::default();

        do_render(&mut terminal, &mut state, &block.render(40));
        block.set_lines(["Header", "Working |", "Footer"]);
        do_render(&mut terminal, &mut state, &block.render(40));

        let vp = terminal.viewport();
        assert_eq!(vp[0], "Header");
        assert!(vp[1].contains("Working"));
        assert_eq!(vp[2], "Footer");
    }
}
