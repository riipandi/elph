use pulldown_cmark::{CodeBlockKind, Tag, TagEnd};

use crate::diff::ansi;

use super::{ListFrame, Renderer, TableFrame};

impl<'a> Renderer<'a> {
    pub(super) fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush_paragraph();
                self.heading_level = Some(level as u8);
            }
            Tag::BlockQuote(_) => {
                self.flush_paragraph();
                self.blockquote_depth += 1;
            }
            Tag::CodeBlock(kind) => {
                self.flush_paragraph();
                self.in_code_block = true;
                self.code_block_lines.clear();
                if let CodeBlockKind::Fenced(lang) = kind
                    && !lang.is_empty()
                {
                    self.code_block_lines
                        .push(self.theme.paint_bullet(&format!("```{lang}")));
                }
            }
            Tag::List(first) => {
                self.flush_paragraph();
                self.list_stack.push(ListFrame {
                    ordered: first.is_some(),
                    next_index: first.unwrap_or(1) as usize,
                    indent: self.list_stack.len(),
                });
            }
            Tag::Item => {
                self.flush_paragraph();
                let marker = if let Some(frame) = self.list_stack.last_mut() {
                    let bullet = if frame.ordered {
                        let n = frame.next_index;
                        frame.next_index += 1;
                        format!("{n}.")
                    } else {
                        "•".to_string()
                    };
                    let indent = "  ".repeat(frame.indent);
                    format!("{indent}{} ", self.theme.paint_bullet(&bullet))
                } else {
                    self.theme.paint_bullet("• ")
                };
                self.current.push_str(&marker);
            }
            Tag::Emphasis => self
                .style
                .push(format!("{}{}", ansi::fg(self.theme.text), ansi::ITALIC)),
            Tag::Strong => self.style.push(format!("{}{}", ansi::fg(self.theme.text), ansi::BOLD)),
            Tag::Strikethrough => self
                .style
                .push(format!("{}{}", ansi::fg(self.theme.text), ansi::STRIKE)),
            Tag::Link { dest_url, .. } => {
                self.link_url = Some(dest_url.to_string());
            }
            Tag::Table(_) => {
                self.flush_paragraph();
                self.table = Some(TableFrame::default());
            }
            Tag::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.in_header = true;
                }
            }
            Tag::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    table.current_row.clear();
                }
            }
            Tag::TableCell => {
                if let Some(table) = self.table.as_mut() {
                    table.in_cell = true;
                }
            }
            _ => {}
        }
    }

    pub(super) fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush_paragraph(),
            TagEnd::Heading(level) => {
                let plain = std::mem::take(&mut self.current);
                let styled = self.theme.paint_heading(level as u8, plain.trim());
                self.push_wrapped_line(styled);
                self.heading_level = None;
            }
            TagEnd::BlockQuote(_) => {
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                self.flush_code_block();
                self.in_code_block = false;
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
                self.flush_paragraph();
            }
            TagEnd::Item => self.flush_paragraph(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.style.pop();
            }
            TagEnd::Link => {
                if let Some(url) = self.link_url.take() {
                    let plain = std::mem::take(&mut self.current);
                    let rendered = if self.use_hyperlinks {
                        self.theme.paint_link(plain.trim(), &url)
                    } else {
                        format!(
                            "{} ({})",
                            self.theme.paint_link(plain.trim(), &url),
                            self.theme.paint_text(&url)
                        )
                    };
                    self.current.push_str(&self.style.apply_after(&rendered));
                }
            }
            TagEnd::TableCell => {
                if let Some(table) = self.table.as_mut() {
                    if !self.current.is_empty() {
                        table.current_row.push(std::mem::take(&mut self.current));
                    }
                    table.in_cell = false;
                }
            }
            TagEnd::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    let row = std::mem::take(&mut table.current_row);
                    if !row.is_empty() {
                        table.rows.push(row);
                    }
                }
            }
            TagEnd::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    let row = std::mem::take(&mut table.current_row);
                    if !row.is_empty() {
                        table.header = Some(row);
                    }
                    table.in_header = false;
                }
            }
            TagEnd::Table => {
                self.flush_table();
            }
            _ => {}
        }
    }
}
