mod tags;

use pulldown_cmark::{Event, Parser};

use crate::utils::{str_display_width, wrap_ansi_line};

use crate::diff::ansi::StylePrefix;
use crate::diff::markdown_table::render_gfm_table_data;

use super::MarkdownTheme;

pub(super) struct ListFrame {
    ordered: bool,
    next_index: usize,
    indent: usize,
}

#[derive(Default)]
pub(super) struct TableFrame {
    header: Option<Vec<String>>,
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    in_header: bool,
    in_cell: bool,
}

pub(super) struct Renderer<'a> {
    pub(super) theme: &'a MarkdownTheme,
    pub(super) width: usize,
    pub(super) use_hyperlinks: bool,
    pub(super) lines: Vec<String>,
    pub(super) current: String,
    pub(super) style: StylePrefix,
    pub(super) list_stack: Vec<ListFrame>,
    pub(super) blockquote_depth: usize,
    pub(super) in_code_block: bool,
    pub(super) code_block_lines: Vec<String>,
    pub(super) link_url: Option<String>,
    pub(super) heading_level: Option<u8>,
    pub(super) table: Option<TableFrame>,
}

impl<'a> Renderer<'a> {
    pub(super) fn new(theme: &'a MarkdownTheme, width: usize, use_hyperlinks: bool) -> Self {
        Self {
            theme,
            width,
            use_hyperlinks,
            lines: Vec::new(),
            current: String::new(),
            style: StylePrefix::default(),
            list_stack: Vec::new(),
            blockquote_depth: 0,
            in_code_block: false,
            code_block_lines: Vec::new(),
            link_url: None,
            heading_level: None,
            table: None,
        }
    }

    pub(super) fn walk(&mut self, parser: Parser<'a>) {
        for event in parser {
            match event {
                Event::Start(tag) => self.start_tag(tag),
                Event::End(tag) => self.end_tag(tag),
                Event::Text(text) => self.push_text(&text),
                Event::Code(text) => {
                    let styled = self.theme.paint_code(&text);
                    self.current.push_str(&self.style.apply_after(&styled));
                }
                Event::SoftBreak => self.current.push(' '),
                Event::HardBreak => self.flush_paragraph(),
                Event::Rule => {
                    self.flush_paragraph();
                    self.lines.push(self.theme.paint_hr(self.width));
                }
                Event::Html(html) => self.push_text(&html),
                _ => {}
            }
        }
        self.flush_paragraph();
        if self.in_code_block {
            self.flush_code_block();
        }
        self.flush_table();
    }

    pub(super) fn push_text(&mut self, text: &str) {
        if self.in_code_block {
            for line in text.lines() {
                self.code_block_lines.push(line.to_string());
            }
            if text.ends_with('\n') {
                self.code_block_lines.push(String::new());
            }
            return;
        }

        let body = if self.heading_level.is_some() {
            text.to_string()
        } else if self.blockquote_depth > 0 {
            self.theme.paint_quote(text)
        } else {
            self.theme.paint_text(text)
        };
        self.current.push_str(&self.style.apply_after(&body));
    }

    pub(super) fn flush_paragraph(&mut self) {
        if self.table.as_ref().is_some_and(|t| t.in_cell) {
            return;
        }
        if self.current.trim().is_empty() {
            self.current.clear();
            return;
        }
        let line = std::mem::take(&mut self.current);
        self.push_wrapped_line(line);
    }

    pub(super) fn flush_table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        let rendered = render_gfm_table_data(table.header, table.rows, self.theme);
        for line in rendered {
            self.push_wrapped_line(line);
        }
    }

    pub(super) fn flush_code_block(&mut self) {
        let block_lines: Vec<String> = self.code_block_lines.drain(..).collect();
        for line in block_lines {
            let body = if line.is_empty() {
                String::new()
            } else {
                format!("  {}", self.theme.paint_codeblock(&line))
            };
            self.push_wrapped_line(body);
        }
        self.lines.push(self.theme.paint_bullet("```"));
    }

    pub(super) fn push_wrapped_line(&mut self, line: String) {
        if str_display_width(&line) <= self.width {
            self.lines.push(line);
            return;
        }
        self.lines.extend(wrap_ansi_line(&line, self.width));
    }

    pub(super) fn finish(self) -> Vec<String> {
        self.lines
    }
}
