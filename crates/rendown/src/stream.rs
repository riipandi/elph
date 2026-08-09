//! Incremental markdown streaming for LLM token deltas.

use std::io::{self, Write};

use crate::ansi::{VisualLine, flatten_visual_lines, write_document_ansi};
use crate::colors::{ColorLevel, detect_color_level, span_anstyle};
use crate::parse::parse_markdown_document_with_theme;
use crate::theme::MarkdownTheme;

/// Streaming markdown → ANSI helper for headless CLI.
///
/// Buffers the full source; on each newline (and on [`finish`](Self::finish)) re-parses and
/// emits only **new** visual lines since the last paint (append-only).
pub struct StreamRenderer {
    source: String,
    width: u16,
    theme: MarkdownTheme,
    /// Number of visual lines already written.
    painted_lines: usize,
    /// Whether any output has been written.
    wrote: bool,
}

impl StreamRenderer {
    pub fn new(width: u16) -> Self {
        Self::with_theme(width, MarkdownTheme::default())
    }

    pub fn with_theme(width: u16, theme: MarkdownTheme) -> Self {
        Self {
            source: String::new(),
            width: width.max(40),
            theme,
            painted_lines: 0,
            wrote: false,
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn wrote_output(&self) -> bool {
        self.wrote
    }

    /// Append a token delta. Emits painted lines when the buffer gains a newline.
    pub fn push(&mut self, delta: &str, out: &mut impl Write) -> io::Result<()> {
        if delta.is_empty() {
            return Ok(());
        }
        let had_newline = delta.contains('\n');
        self.source.push_str(delta);
        if had_newline {
            self.repaint_new_lines(out)?;
        }
        Ok(())
    }

    /// Flush remaining incomplete tail (re-parse full source, emit remaining lines).
    pub fn finish(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.source.is_empty() && self.painted_lines == 0 {
            return Ok(());
        }
        self.repaint_new_lines(out)?;
        // If we never emitted a trailing newline after the last visual line, ensure flush.
        out.flush()?;
        Ok(())
    }

    fn repaint_new_lines(&mut self, out: &mut impl Write) -> io::Result<()> {
        let doc = parse_markdown_document_with_theme(&self.source, &self.theme);
        let visual = flatten_visual_lines(&doc, self.width, &self.theme);

        // Streaming incomplete fences: if the source ends mid-fence without closing ```,
        // pulldown still may parse partial content — we just paint whatever we have.
        if visual.len() < self.painted_lines {
            // Layout regression (rare): full re-emit.
            self.painted_lines = 0;
            // Clear is not possible on plain TTY stream; re-print all with marker would
            // scramble. Accept append-only of full document for v1 if shrink happens.
            // Reset and rewrite from scratch by writing a blank separator then full doc.
            writeln!(out)?;
            write_document_ansi(&doc, self.width, &self.theme, out)?;
            self.painted_lines = visual.len();
            self.wrote = true;
            return out.flush();
        }

        for line in visual.iter().skip(self.painted_lines) {
            write_visual_line_ansi(line, &self.theme, out)?;
            writeln!(out)?;
            self.wrote = true;
        }
        self.painted_lines = visual.len();
        out.flush()
    }
}

fn write_visual_line_ansi(line: &VisualLine, theme: &MarkdownTheme, out: &mut impl Write) -> io::Result<()> {
    if line.spans.is_empty() || line.spans.iter().all(|s| s.text.is_empty()) {
        return Ok(());
    }
    let bg = if line.code_background && detect_color_level() != ColorLevel::None {
        Some(anstyle::RgbColor(theme.code_bg.r, theme.code_bg.g, theme.code_bg.b))
    } else {
        None
    };
    for span in &line.spans {
        if span.text.is_empty() {
            continue;
        }
        let mut style = span_anstyle(span);
        if let Some(bg) = bg {
            style = style.bg_color(Some(anstyle::Color::Rgb(bg)));
        }
        if let Some(href) = &span.href {
            write!(out, "\x1b]8;;{href}\x1b\\")?;
            write!(out, "{style}{}{style:#}", span.text)?;
            write!(out, "\x1b]8;;\x1b\\")?;
        } else {
            write!(out, "{style}{}{style:#}", span.text)?;
        }
    }
    Ok(())
}

/// Detect terminal width (columns), defaulting to 80.
pub fn terminal_width() -> u16 {
    crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80).max(40)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_heading_across_chunks() {
        let mut r = StreamRenderer::new(80);
        let mut buf = Vec::new();
        r.push("# Hel", &mut buf).unwrap();
        assert!(buf.is_empty(), "partial line should not emit");
        r.push("lo\n\nbody\n", &mut buf).unwrap();
        r.finish(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("Hello"));
        assert!(s.contains("body"));
    }

    #[test]
    fn finish_flushes_without_trailing_newline() {
        let mut r = StreamRenderer::new(80);
        let mut buf = Vec::new();
        r.push("plain text no nl", &mut buf).unwrap();
        assert!(buf.is_empty());
        r.finish(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("plain text no nl"));
    }
}
