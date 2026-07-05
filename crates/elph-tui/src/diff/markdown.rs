use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::utils::{pad_lines, str_display_width, wrap_ansi_line};

use super::ansi::{self, StylePrefix, hyperlink, styled};
use super::component::{Line, LineComponent};

/// ANSI palette for markdown rendering.
#[derive(Debug, Clone, Copy)]
pub struct MarkdownTheme {
    pub heading: u8,
    pub text: u8,
    pub link: u8,
    pub code: u8,
    pub code_block: u8,
    pub quote: u8,
    pub quote_border: u8,
    pub hr: u8,
    pub list_bullet: u8,
}

impl MarkdownTheme {
    pub fn dark() -> Self {
        Self {
            heading: 51,
            text: 252,
            link: 39,
            code: 203,
            code_block: 252,
            quote: 245,
            quote_border: 240,
            hr: 240,
            list_bullet: 250,
        }
    }

    pub fn light() -> Self {
        Self {
            heading: 25,
            text: 238,
            link: 25,
            code: 161,
            code_block: 238,
            quote: 244,
            quote_border: 250,
            hr: 250,
            list_bullet: 240,
        }
    }

    fn paint_text(&self, text: &str) -> String {
        styled(&ansi::fg(self.text), text)
    }

    fn paint_heading(&self, level: u8, text: &str) -> String {
        let prefix = if level <= 1 {
            format!("{}{}{}", ansi::fg(self.heading), ansi::BOLD, ansi::UNDERLINE)
        } else {
            format!("{}{}", ansi::fg(self.heading), ansi::BOLD)
        };
        styled(&prefix, text)
    }

    fn paint_code(&self, text: &str) -> String {
        styled(&format!("{}{}", ansi::fg(self.code), ansi::BOLD), text)
    }

    fn paint_codeblock(&self, text: &str) -> String {
        styled(&format!("{}{}", ansi::fg(self.code_block), ansi::DIM), text)
    }

    fn paint_link(&self, text: &str, url: &str) -> String {
        let body = styled(&format!("{}{}", ansi::fg(self.link), ansi::UNDERLINE), text);
        hyperlink(url, &body)
    }

    fn paint_quote(&self, text: &str) -> String {
        styled(&format!("{}{}", ansi::fg(self.quote), ansi::ITALIC), text)
    }

    fn paint_hr(&self, width: usize) -> String {
        let len = width.clamp(3, 80);
        styled(&ansi::fg(self.hr), &"─".repeat(len))
    }

    fn paint_bullet(&self, marker: &str) -> String {
        styled(&ansi::fg(self.list_bullet), marker)
    }
}

/// Renders markdown to ANSI terminal lines (pi-tui `Markdown`).
pub struct Markdown {
    text: String,
    padding_x: u16,
    padding_y: u16,
    theme: MarkdownTheme,
    use_hyperlinks: bool,
    cache_key: Option<(String, u16)>,
    cache_lines: Vec<Line>,
}

impl Markdown {
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_theme(text, MarkdownTheme::dark())
    }

    pub fn with_theme(text: impl Into<String>, theme: MarkdownTheme) -> Self {
        Self {
            text: text.into(),
            padding_x: 0,
            padding_y: 0,
            theme,
            use_hyperlinks: true,
            cache_key: None,
            cache_lines: Vec::new(),
        }
    }

    pub fn with_padding(mut self, padding_x: u16, padding_y: u16) -> Self {
        self.padding_x = padding_x;
        self.padding_y = padding_y;
        self
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.invalidate();
    }

    pub fn set_use_hyperlinks(&mut self, enabled: bool) {
        self.use_hyperlinks = enabled;
        self.invalidate();
    }

    fn build_lines(&self, width: u16) -> Vec<Line> {
        let width = width.max(1) as usize;
        if self.text.trim().is_empty() {
            return Vec::new();
        }

        let normalized = self.text.replace('\t', "   ");
        let content_width = width.saturating_sub(self.padding_x as usize).max(1);
        let mut renderer = Renderer::new(&self.theme, content_width, self.use_hyperlinks);
        let parser = Parser::new_ext(&normalized, Options::all());
        renderer.walk(parser);
        let wrapped = renderer.finish();
        pad_lines(&wrapped, self.padding_x as usize, self.padding_y as usize)
    }
}

impl LineComponent for Markdown {
    fn render(&mut self, width: u16) -> Vec<Line> {
        let key = (self.text.clone(), width);
        if self.cache_key.as_ref() == Some(&key) {
            return self.cache_lines.clone();
        }
        let lines = self.build_lines(width);
        self.cache_key = Some(key);
        self.cache_lines = lines.clone();
        lines
    }

    fn invalidate(&mut self) {
        self.cache_key = None;
        self.cache_lines.clear();
    }
}

struct ListFrame {
    ordered: bool,
    next_index: usize,
    indent: usize,
}

struct Renderer<'a> {
    theme: &'a MarkdownTheme,
    width: usize,
    use_hyperlinks: bool,
    lines: Vec<String>,
    current: String,
    style: StylePrefix,
    list_stack: Vec<ListFrame>,
    blockquote_depth: usize,
    in_code_block: bool,
    code_block_lines: Vec<String>,
    link_url: Option<String>,
    heading_level: Option<u8>,
}

impl<'a> Renderer<'a> {
    fn new(theme: &'a MarkdownTheme, width: usize, use_hyperlinks: bool) -> Self {
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
        }
    }

    fn walk(&mut self, parser: Parser<'a>) {
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
    }

    fn start_tag(&mut self, tag: Tag<'_>) {
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
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
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
            _ => {}
        }
    }

    fn push_text(&mut self, text: &str) {
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

    fn flush_paragraph(&mut self) {
        if self.current.trim().is_empty() {
            self.current.clear();
            return;
        }
        let line = std::mem::take(&mut self.current);
        self.push_wrapped_line(line);
    }

    fn flush_code_block(&mut self) {
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

    fn push_wrapped_line(&mut self, line: String) {
        if str_display_width(&line) <= self.width {
            self.lines.push(line);
            return;
        }
        self.lines.extend(wrap_ansi_line(&line, self.width));
    }

    fn finish(self) -> Vec<String> {
        self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_heading_and_bold() {
        let mut md = Markdown::new("# Title\n\nHello **world**");
        let lines = md.render(40);
        assert!(!lines.is_empty());
        let joined = lines.join("\n");
        assert!(joined.contains("Title"));
        assert!(joined.contains("world"));
    }

    #[test]
    fn renders_code_block() {
        let mut md = Markdown::new("```rs\nlet x = 1;\n```");
        let lines = md.render(40);
        let joined = lines.join("\n");
        assert!(joined.contains("let x = 1;"));
    }
}
