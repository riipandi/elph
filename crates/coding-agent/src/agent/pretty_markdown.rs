//! Streaming CommonMark/markdown pretty-print for `elph run --output=pretty`.
//!
//! Uses [`rendown`] (cloned elph-tui markdown pipeline → ANSI).

use std::io::{self, IsTerminal, Stdout, Write};

use rendown::{StreamRenderer, terminal_width};

/// Incremental markdown → ANSI terminal sink for LLM token streams.
///
/// Buffers source and re-parses on newlines (and finish) via [`StreamRenderer`].
/// When stdout is not a TTY or `NO_COLOR` is set, falls back to raw passthrough.
pub struct PrettyMarkdownSink {
    pretty: bool,
    wrote: bool,
    stdout: Stdout,
    renderer: Option<StreamRenderer>,
}

impl PrettyMarkdownSink {
    /// Prefer pretty only when stdout is a TTY; otherwise emit raw tokens.
    pub fn new() -> Self {
        let pretty = io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let width = if pretty { terminal_width() } else { 80 };
        let stdout = io::stdout();
        let renderer = if pretty { Some(StreamRenderer::new(width)) } else { None };
        Self {
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

        let Some(renderer) = self.renderer.as_mut() else {
            return Ok(());
        };
        renderer.push(text, &mut self.stdout)?;
        if renderer.wrote_output() {
            self.wrote = true;
        }
        Ok(())
    }

    /// Flush incomplete tail and finalize open markdown.
    pub fn finish(&mut self) -> io::Result<()> {
        if !self.pretty {
            return Ok(());
        }

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.finish(&mut self.stdout)?;
            if renderer.wrote_output() {
                self.wrote = true;
            }
        }
        self.stdout.flush()?;
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
    use rendown::StreamRenderer;

    #[test]
    fn buffers_partial_line_until_newline() {
        let mut r = StreamRenderer::new(80);
        let mut buf = Vec::new();
        r.push("# Hel", &mut buf).unwrap();
        assert!(buf.is_empty());
        r.push("lo\n", &mut buf).unwrap();
        assert!(!buf.is_empty());
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("Hello") || s.contains("Hel"));
    }

    #[test]
    fn non_pretty_new_defaults_respect_no_tty_logic() {
        // Construction must not panic without a TTY.
        let _ = PrettyMarkdownSink::new();
    }
}
