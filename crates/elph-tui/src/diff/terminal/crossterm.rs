use anyhow::Result;
use std::io::Write;

use crossterm::{
    cursor, queue,
    terminal::{self, ClearType},
};

use super::{Terminal, open_tui_writer};

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
