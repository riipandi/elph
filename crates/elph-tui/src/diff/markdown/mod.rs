mod renderer;

use crate::utils::pad_lines;

use super::ansi::{self, hyperlink, styled};
use super::component::{Line, LineComponent};
use renderer::Renderer;

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

    pub(crate) fn paint_text(&self, text: &str) -> String {
        styled(&ansi::fg(self.text), text)
    }

    pub(super) fn paint_heading(&self, level: u8, text: &str) -> String {
        let prefix = if level <= 1 {
            format!("{}{}{}", ansi::fg(self.heading), ansi::BOLD, ansi::UNDERLINE)
        } else {
            format!("{}{}", ansi::fg(self.heading), ansi::BOLD)
        };
        styled(&prefix, text)
    }

    pub(super) fn paint_code(&self, text: &str) -> String {
        styled(&format!("{}{}", ansi::fg(self.code), ansi::BOLD), text)
    }

    pub(super) fn paint_codeblock(&self, text: &str) -> String {
        styled(&format!("{}{}", ansi::fg(self.code_block), ansi::DIM), text)
    }

    pub(super) fn paint_link(&self, text: &str, url: &str) -> String {
        let body = styled(&format!("{}{}", ansi::fg(self.link), ansi::UNDERLINE), text);
        hyperlink(url, &body)
    }

    pub(super) fn paint_quote(&self, text: &str) -> String {
        styled(&format!("{}{}", ansi::fg(self.quote), ansi::ITALIC), text)
    }

    pub(super) fn paint_hr(&self, width: usize) -> String {
        let len = width.clamp(3, 80);
        styled(&ansi::fg(self.hr), &"─".repeat(len))
    }

    pub(super) fn paint_bullet(&self, marker: &str) -> String {
        styled(&ansi::fg(self.list_bullet), marker)
    }
}

/// Renders markdown to ANSI terminal lines.
pub struct Markdown {
    text: String,
    padding_x: u16,
    padding_y: u16,
    theme: MarkdownTheme,
    use_hyperlinks: bool,
    cache_width: Option<u16>,
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
            cache_width: None,
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
        let parser = pulldown_cmark::Parser::new_ext(&normalized, pulldown_cmark::Options::all());
        renderer.walk(parser);
        let wrapped = renderer.finish();
        pad_lines(&wrapped, self.padding_x as usize, self.padding_y as usize)
    }
}

/// Renders markdown to ANSI lines without maintaining component state.
pub fn render_markdown_lines(text: &str, width: u16, theme: MarkdownTheme) -> Vec<Line> {
    let mut md = Markdown::with_theme(text, theme);
    md.render(width)
}

impl LineComponent for Markdown {
    fn render(&mut self, width: u16) -> Vec<Line> {
        if self.cache_width == Some(width) && !self.cache_lines.is_empty() {
            return self.cache_lines.clone();
        }
        let lines = self.build_lines(width);
        self.cache_width = Some(width);
        self.cache_lines = lines.clone();
        lines
    }

    fn invalidate(&mut self) {
        self.cache_width = None;
        self.cache_lines.clear();
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

    #[test]
    fn renders_gfm_table() {
        let md = "| Name | Age |\n|------|-----|\n| Alice | 30 |\n| Bob | 25 |";
        let lines = render_markdown_lines(md, 50, MarkdownTheme::dark());
        let joined = lines.join("\n");
        assert!(joined.contains("Name"));
        assert!(joined.contains("Alice"));
        assert!(joined.contains('┌'));
        assert!(joined.contains('┘'));
    }
}
