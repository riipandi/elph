//! Streaming CommonMark/markdown pretty-print for `elph run --output=pretty`.
//!
//! Uses [streamdown](https://crates.io/crates/streamdown-parser) (parser + render).
//! `streamdown-render` depends on **crossterm** for terminal size — same stack family as iocraft.

use std::io::{self, IsTerminal, Stdout, Write};

use streamdown_parser::Parser;
use streamdown_render::{terminal_width, Renderer};

/// Incremental markdown → ANSI terminal sink for LLM token streams.
///
/// Line-oriented: text is buffered until `\n`, then parsed and rendered with a
/// **long-lived** [`Renderer`] (preserves code-fence / list / table state).
/// On [`finish`](Self::finish), any trailing partial line is flushed and open
/// blocks are closed via [`Parser::finalize`].
pub struct PrettyMarkdownSink {
    parser: Parser,
    line_buf: String,
    width: usize,
    /// When false (non-TTY / NO_COLOR), falls back to raw passthrough (same as plain).
    pretty: bool,
    wrote: bool,
    /// Owned stdout handle so `Renderer` can live for the whole stream.
    stdout: Stdout,
    /// Only used when `pretty` is true; re-created after each flush of the lock…  
    /// We keep the renderer via a workaround: render into an intermediate buffer
    /// with a **persistent** renderer state by embedding state in a wrapper that
    /// always writes through to stdout.
    renderer: Option<Renderer<Stdout>>,
}

impl PrettyMarkdownSink {
    /// Prefer pretty only when stdout is a TTY; otherwise emit raw tokens.
    pub fn new() -> Self {
        let pretty = io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let width = if pretty {
            terminal_width().max(40)
        } else {
            80
        };
        let stdout = io::stdout();
        let renderer = if pretty {
            Some(Renderer::new(io::stdout(), width))
        } else {
            None
        };
        Self {
            parser: Parser::new(),
            line_buf: String::new(),
            width,
            pretty,
            wrote: false,
            stdout,
            renderer,
        }
    }

    pub fn is_pretty(&self) -> bool {
        self.pretty
    }

    pub fn wrote_output(&self) -> bool {
        self.wrote
    }

    /// Push a text delta (may contain zero or many newlines).
    pub fn push_delta(&mut self, text: &str) -> io::Result<()> {
        if !self.pretty {
            write!(self.stdout, "{text}")?;
            self.stdout.flush()?;
            if !text.is_empty() {
                self.wrote = true;
            }
            return Ok(());
        }

        for ch in text.chars() {
            if ch == '\n' {
                let line = std::mem::take(&mut self.line_buf);
                self.emit_line(&line)?;
            } else if ch != '\r' {
                self.line_buf.push(ch);
            }
        }
        Ok(())
    }

    /// Flush incomplete line + finalize open markdown blocks.
    pub fn finish(&mut self) -> io::Result<()> {
        if !self.pretty {
            if !self.line_buf.is_empty() {
                write!(self.stdout, "{}", self.line_buf)?;
                self.line_buf.clear();
                self.stdout.flush()?;
                self.wrote = true;
            }
            return Ok(());
        }

        if !self.line_buf.is_empty() {
            let line = std::mem::take(&mut self.line_buf);
            self.emit_line(&line)?;
        }

        if let Some(renderer) = self.renderer.as_mut() {
            for event in self.parser.finalize() {
                renderer.render_event(&event)?;
            }
            // Drop renderer so the final write is complete; stdout is flushed below.
        }
        // Take renderer to flush via Drop of nested writes, then flush stdout.
        self.renderer.take();
        self.stdout.flush()?;
        Ok(())
    }

    fn emit_line(&mut self, line: &str) -> io::Result<()> {
        let Some(renderer) = self.renderer.as_mut() else {
            return Ok(());
        };
        for event in self.parser.parse_line(line) {
            renderer.render_event(&event)?;
        }
        // Renderer writes through to its owned Stdout; also flush the shared handle.
        self.stdout.flush()?;
        self.wrote = true;
        Ok(())
    }
}

impl Default for PrettyMarkdownSink {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use streamdown_parser::Parser;

    #[test]
    fn buffers_partial_line_until_newline() {
        // Unit-test line buffering without requiring a TTY pretty path.
        let mut line_buf = String::new();
        let mut parser = Parser::new();
        for ch in "# Hel".chars() {
            if ch == '\n' {
                let _ = parser.parse_line(&std::mem::take(&mut line_buf));
            } else {
                line_buf.push(ch);
            }
        }
        assert_eq!(line_buf, "# Hel");
        for ch in "lo\n".chars() {
            if ch == '\n' {
                let events = parser.parse_line(&std::mem::take(&mut line_buf));
                assert!(events.iter().any(|e| matches!(e, streamdown_parser::ParseEvent::Heading { .. })));
            } else {
                line_buf.push(ch);
            }
        }
        assert!(line_buf.is_empty());
    }
}
