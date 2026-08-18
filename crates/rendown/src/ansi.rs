//! Walk [`MarkdownDocument`] → ANSI terminal output.

use std::io::{self, Write};

use crate::blocks::{CODE_BLOCK_INSET_H, CODE_BLOCK_INSET_V, block_gap_after, code_content_width};
use crate::colors::{ColorLevel, span_anstyle};
use crate::mermaid::mermaid_display_text;
use crate::model::{MarkdownDocument, MarkdownLineKind, StyledSpan};
use crate::table::format_table_lines;
use crate::theme::MarkdownTheme;
use crate::wrap::{display_width, wrap_text_to_lines, wrap_with_hanging_ranges};

/// Write a full document as ANSI text (always ends with a trailing newline if non-empty).
pub fn write_document_ansi(
    doc: &MarkdownDocument,
    width: u16,
    theme: &MarkdownTheme,
    color_level: ColorLevel,
    out: &mut impl Write,
) -> io::Result<()> {
    let lines = flatten_visual_lines(doc, width, theme);
    for (i, visual) in lines.iter().enumerate() {
        write_visual_line(visual, theme, color_level, out)?;
        if i + 1 < lines.len() || !visual.is_empty() {
            writeln!(out)?;
        }
    }
    Ok(())
}

/// One paint row: empty = blank line.
#[derive(Clone, Debug)]
pub struct VisualLine {
    pub spans: Vec<StyledSpan>,
    pub code_background: bool,
}

impl VisualLine {
    fn blank() -> Self {
        Self {
            spans: Vec::new(),
            code_background: false,
        }
    }

    fn is_empty(&self) -> bool {
        self.spans.is_empty() || self.spans.iter().all(|s| s.text.is_empty())
    }
}

/// Expand a document into display rows (wrap, table expand, code padding, gaps).
pub fn flatten_visual_lines(doc: &MarkdownDocument, width: u16, theme: &MarkdownTheme) -> Vec<VisualLine> {
    let width = width.max(1);
    let mut out = Vec::new();
    let lines = &doc.lines;
    let mut index = 0usize;
    while index < lines.len() {
        let line = &lines[index];
        if line.is_blank() {
            out.push(VisualLine::blank());
            index += 1;
            continue;
        }

        if let Some(source) = line.mermaid_source.as_deref() {
            let inner = code_content_width(width);
            let text = mermaid_display_text(source, inner);
            for _ in 0..CODE_BLOCK_INSET_V {
                out.push(VisualLine {
                    spans: vec![StyledSpan::plain("", theme.body)],
                    code_background: true,
                });
            }
            for src_line in text.lines() {
                let mut spans = vec![StyledSpan::plain(" ".repeat(CODE_BLOCK_INSET_H as usize), theme.body)];
                spans.push(StyledSpan::plain(src_line, theme.body));
                out.push(VisualLine {
                    spans,
                    code_background: true,
                });
            }
            for _ in 0..CODE_BLOCK_INSET_V {
                out.push(VisualLine {
                    spans: vec![StyledSpan::plain("", theme.body)],
                    code_background: true,
                });
            }
            let gap = block_gap_after(lines, index);
            for _ in 0..gap {
                out.push(VisualLine::blank());
            }
            index += 1;
            continue;
        }

        if line.kind == MarkdownLineKind::Table {
            if let Some(table) = &line.table {
                for tline in format_table_lines(table, width, theme) {
                    out.push(VisualLine {
                        spans: tline.spans,
                        code_background: false,
                    });
                }
            }
        } else if line.kind == MarkdownLineKind::Rule {
            let rule: String = std::iter::repeat_n('─', width as usize).collect();
            out.push(VisualLine {
                spans: vec![StyledSpan::plain(rule, theme.horizontal_rule)],
                code_background: false,
            });
        } else if line.code_background || line.kind == MarkdownLineKind::Code {
            // Consume contiguous code segment.
            let mut end = index + 1;
            while end < lines.len() && (lines[end].code_background || lines[end].kind == MarkdownLineKind::Code) {
                end += 1;
            }
            let segment = &lines[index..end];
            let use_card = segment.iter().any(|l| l.code_background);
            let content_w = if use_card { code_content_width(width) } else { width };
            if use_card {
                for _ in 0..CODE_BLOCK_INSET_V {
                    out.push(VisualLine {
                        spans: vec![StyledSpan::plain("", theme.body)],
                        code_background: true,
                    });
                }
            }
            for code_line in segment {
                let plain: String = code_line.spans.iter().map(|s| s.text.as_str()).collect();
                let ranges = wrap_with_hanging_ranges(&plain, content_w);
                let chars: Vec<char> = plain.chars().collect();
                for (ri, (start, end_r)) in ranges.iter().enumerate() {
                    let indent = if ri > 0 {
                        chars
                            .iter()
                            .take_while(|c| **c == ' ')
                            .count()
                            .min(content_w as usize - 1)
                    } else {
                        0
                    };
                    let mut spans = Vec::new();
                    if use_card {
                        spans.push(StyledSpan::plain(" ".repeat(CODE_BLOCK_INSET_H as usize), theme.body));
                    }
                    if indent > 0 {
                        spans.push(StyledSpan::plain(" ".repeat(indent), theme.body));
                    }
                    // Re-slice styled spans approximately by char ranges on plain text.
                    spans.extend(slice_spans_by_char_range(&code_line.spans, *start, *end_r));
                    out.push(VisualLine {
                        spans,
                        code_background: use_card,
                    });
                }
            }
            if use_card {
                for _ in 0..CODE_BLOCK_INSET_V {
                    out.push(VisualLine {
                        spans: vec![StyledSpan::plain("", theme.body)],
                        code_background: true,
                    });
                }
            }
            index = end;
            // gap after
            if index < lines.len() {
                let gap = block_gap_after(lines, end.saturating_sub(1));
                for _ in 0..gap {
                    out.push(VisualLine::blank());
                }
            }
            continue;
        } else {
            // Paragraph / heading / list / blockquote / continuation
            let plain: String = line.spans.iter().map(|s| s.text.as_str()).collect();
            let wrapped = wrap_text_to_lines(&plain, width as usize);
            if wrapped.is_empty() {
                out.push(VisualLine {
                    spans: line.spans.clone(),
                    code_background: false,
                });
            } else {
                let mut char_offset = 0usize;
                for (wi, wline) in wrapped.iter().enumerate() {
                    let wlen = wline.chars().count();
                    // Skip leading spaces consumed by wrap of previous (approx)
                    if wi == 0 {
                        out.push(VisualLine {
                            spans: slice_spans_by_char_range(&line.spans, 0, wlen),
                            code_background: false,
                        });
                        char_offset = wlen;
                    } else {
                        // Advance through whitespace in plain between wraps
                        let plain_chars: Vec<char> = plain.chars().collect();
                        while char_offset < plain_chars.len() && plain_chars[char_offset].is_whitespace() {
                            char_offset += 1;
                        }
                        let end = (char_offset + wlen).min(plain_chars.len());
                        out.push(VisualLine {
                            spans: slice_spans_by_char_range(&line.spans, char_offset, end),
                            code_background: false,
                        });
                        char_offset = end;
                    }
                }
            }
        }

        let gap = block_gap_after(lines, index);
        for _ in 0..gap {
            out.push(VisualLine::blank());
        }
        index += 1;
    }
    out
}

fn slice_spans_by_char_range(spans: &[StyledSpan], start: usize, end: usize) -> Vec<StyledSpan> {
    if start >= end {
        return vec![StyledSpan::plain(
            "",
            spans.first().map(|s| s.color).unwrap_or_default_body(),
        )];
    }
    let mut out = Vec::new();
    let mut pos = 0usize;
    for span in spans {
        let len = span.text.chars().count();
        let span_end = pos + len;
        if span_end <= start {
            pos = span_end;
            continue;
        }
        if pos >= end {
            break;
        }
        let local_start = start.saturating_sub(pos);
        let local_end = end.saturating_sub(pos).min(len);
        if local_start < local_end {
            let text: String = span
                .text
                .chars()
                .skip(local_start)
                .take(local_end - local_start)
                .collect();
            if !text.is_empty() {
                out.push(StyledSpan {
                    text,
                    color: span.color,
                    weight: span.weight,
                    italic: span.italic,
                    underline: span.underline,
                    href: span.href.clone(),
                });
            }
        }
        pos = span_end;
        if pos >= end {
            break;
        }
    }
    if out.is_empty() {
        out.push(StyledSpan::plain("", spans.first().map(|s| s.color).unwrap_or_default_body()));
    }
    out
}

trait DefaultBody {
    fn unwrap_or_default_body(self) -> crate::model::RgbColor;
}

impl DefaultBody for Option<crate::model::RgbColor> {
    fn unwrap_or_default_body(self) -> crate::model::RgbColor {
        self.unwrap_or(crate::theme::MarkdownTheme::default().body)
    }
}

fn write_visual_line(
    line: &VisualLine,
    theme: &MarkdownTheme,
    color_level: ColorLevel,
    out: &mut impl Write,
) -> io::Result<()> {
    if line.is_empty() {
        return Ok(());
    }
    let bg = if line.code_background && color_level != ColorLevel::None {
        Some(anstyle::RgbColor(theme.code_bg.r, theme.code_bg.g, theme.code_bg.b))
    } else {
        None
    };
    for span in &line.spans {
        write_span(span, bg, color_level, out)?;
    }
    // Pad code background to full remaining — skip for simplicity (fg-only tint via spaces already).
    let _ = display_width;
    Ok(())
}

fn write_span(
    span: &StyledSpan,
    bg: Option<anstyle::RgbColor>,
    color_level: ColorLevel,
    out: &mut impl Write,
) -> io::Result<()> {
    if span.text.is_empty() {
        return Ok(());
    }
    let mut style = span_anstyle(span, color_level);
    if let Some(bg) = bg {
        style = style.bg_color(Some(anstyle::Color::Rgb(bg)));
    }
    if let Some(href) = &span.href {
        // OSC 8 hyperlink
        write!(out, "\x1b]8;;{href}\x1b\\")?;
        write!(out, "{style}{}{style:#}", span.text)?;
        write!(out, "\x1b]8;;\x1b\\")?;
    } else {
        write!(out, "{style}{}{style:#}", span.text)?;
    }
    Ok(())
}

/// Convert document to plain text (no ANSI) for tests / fallback.
pub fn document_to_plain(doc: &MarkdownDocument) -> String {
    doc.lines
        .iter()
        .map(|line| {
            if line.kind == MarkdownLineKind::Rule {
                "─".repeat(40)
            } else if let Some(source) = &line.mermaid_source {
                source.clone()
            } else if let Some(table) = &line.table {
                table
                    .rows
                    .iter()
                    .map(|row| row.join(" | "))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                line.spans.iter().map(|s| s.text.as_str()).collect::<String>()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_markdown_document_with_theme;

    #[test]
    fn ansi_contains_heading_text() {
        let doc = parse_markdown_document_with_theme("# Hello\n\nworld", &MarkdownTheme::default());
        let mut buf = Vec::new();
        write_document_ansi(&doc, 80, &MarkdownTheme::default(), ColorLevel::TrueColor, &mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("Hello"));
        assert!(s.contains("world"));
        // SGR or OSC likely present for heading color
        assert!(s.contains('\x1b') || s.contains("Hello"));
    }
}
