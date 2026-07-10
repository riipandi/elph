mod crossterm;
mod recording;

use anyhow::Result;
use std::io::{self, IsTerminal, Write};

pub use crossterm::CrosstermTerminal;
pub use recording::RecordingTerminal;

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
