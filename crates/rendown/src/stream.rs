//! Incremental markdown streaming for LLM token deltas.

use std::io::{self, Write};

use crate::ansi::{VisualLine, flatten_visual_lines, write_document_ansi};
use crate::builder::Rendown;
use crate::colors::{ColorLevel, span_anstyle};
use crate::parse::parse_markdown_document_with_theme;
use crate::theme::MarkdownTheme;

/// Streaming markdown → ANSI helper.
///
/// Buffers the full source; on each newline (and on [`finish`](Self::finish)) re-parses and
/// emits only **new** visual lines since the last paint (append-only).
pub struct StreamRenderer {
    source: String,
    width: u16,
    theme: MarkdownTheme,
    color_level: ColorLevel,
    painted_lines: usize,
    wrote: bool,
}

impl StreamRenderer {
    pub fn from_rendown(rendown: &Rendown) -> Self {
        Self {
            source: String::new(),
            width: rendown.width_value(),
            theme: *rendown.theme_ref(),
            color_level: rendown.resolved_color_level(),
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
            self.repaint_new_lines(out).inspect_err(|err| {
                log::warn!("rendown stream paint failed: {err}");
            })?;
        }
        Ok(())
    }

    /// Flush remaining incomplete tail.
    pub fn finish(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.source.is_empty() && self.painted_lines == 0 {
            return Ok(());
        }
        self.repaint_new_lines(out).inspect_err(|err| {
            log::warn!("rendown stream finish failed: {err}");
        })?;
        out.flush()?;
        Ok(())
    }

    fn repaint_new_lines(&mut self, out: &mut impl Write) -> io::Result<()> {
        let doc = parse_markdown_document_with_theme(&self.source, &self.theme);
        let visual = flatten_visual_lines(&doc, self.width, &self.theme);

        if visual.len() < self.painted_lines {
            self.painted_lines = 0;
            writeln!(out)?;
            write_document_ansi(&doc, self.width, &self.theme, self.color_level, out)?;
            self.painted_lines = visual.len();
            self.wrote = true;
            return out.flush();
        }

        for line in visual.iter().skip(self.painted_lines) {
            write_visual_line_ansi(line, &self.theme, self.color_level, out)?;
            writeln!(out)?;
            self.wrote = true;
        }
        self.painted_lines = visual.len();
        out.flush()
    }
}

fn write_visual_line_ansi(
    line: &VisualLine,
    theme: &MarkdownTheme,
    color_level: ColorLevel,
    out: &mut impl Write,
) -> io::Result<()> {
    if line.spans.is_empty() || line.spans.iter().all(|s| s.text.is_empty()) {
        return Ok(());
    }
    let bg = if line.code_background && color_level != ColorLevel::None {
        Some(anstyle::RgbColor(theme.code_bg.r, theme.code_bg.g, theme.code_bg.b))
    } else {
        None
    };
    for span in &line.spans {
        if span.text.is_empty() {
            continue;
        }
        let mut style = span_anstyle(span, color_level);
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
        let mut r = Rendown::new().width(80).stream();
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
        let mut r = Rendown::new().width(80).stream();
        let mut buf = Vec::new();
        r.push("plain text no nl", &mut buf).unwrap();
        assert!(buf.is_empty());
        r.finish(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("plain text no nl"));
    }

    #[test]
    fn stream_honors_requested_width() {
        let stream = Rendown::new().width(20).stream();
        assert_eq!(stream.width(), 20);
    }
}
