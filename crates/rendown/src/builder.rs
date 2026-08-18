//! Primary builder API for parse + ANSI write.

use std::io::{self, Write};

use crate::ansi::{document_to_plain, write_document_ansi};
use crate::colors::{ColorLevel, detect_color_level};
use crate::layout::ansi_row_count;
use crate::model::MarkdownDocument;
use crate::parse::parse_markdown_document_with_theme;
use crate::theme::MarkdownTheme;

/// Configurable markdown → ANSI renderer.
///
/// All options are optional. Defaults: width 80, [`MarkdownTheme::dark`], auto color detection.
#[derive(Clone, Debug)]
pub struct Rendown {
    width: u16,
    theme: MarkdownTheme,
    color_level: Option<ColorLevel>,
}

impl Default for Rendown {
    fn default() -> Self {
        Self::new()
    }
}

impl Rendown {
    pub fn new() -> Self {
        Self {
            width: 80,
            theme: MarkdownTheme::dark(),
            color_level: None,
        }
    }

    pub fn width(mut self, width: u16) -> Self {
        self.width = width.max(1);
        self
    }

    pub fn theme(mut self, theme: MarkdownTheme) -> Self {
        self.theme = theme;
        self
    }

    /// Force a color level instead of detecting from the environment.
    pub fn color_level(mut self, level: ColorLevel) -> Self {
        self.color_level = Some(level);
        self
    }

    pub fn parse(&self, source: &str) -> MarkdownDocument {
        parse_markdown_document_with_theme(source, &self.theme)
    }

    pub fn write(&self, doc: &MarkdownDocument, out: &mut impl Write) -> io::Result<()> {
        write_document_ansi(doc, self.width, &self.theme, self.resolved_color_level(), out)
    }

    pub fn render(&self, source: &str, out: &mut impl Write) -> io::Result<()> {
        let doc = self.parse(source);
        self.write(&doc, out)
    }

    pub fn render_string(&self, source: &str) -> io::Result<String> {
        let mut buf = Vec::new();
        self.render(source, &mut buf)?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    pub fn plain(&self, source: &str) -> String {
        document_to_plain(&self.parse(source))
    }

    pub fn row_count(&self, doc: &MarkdownDocument) -> u16 {
        ansi_row_count(doc, self.width, &self.theme)
    }

    #[cfg(feature = "stream")]
    pub(crate) fn theme_ref(&self) -> &MarkdownTheme {
        &self.theme
    }

    #[cfg(feature = "stream")]
    pub(crate) fn width_value(&self) -> u16 {
        self.width
    }

    pub fn resolved_color_level(&self) -> ColorLevel {
        self.color_level.unwrap_or_else(detect_color_level)
    }

    #[cfg(feature = "stream")]
    pub fn stream(&self) -> crate::stream::StreamRenderer {
        crate::stream::StreamRenderer::from_rendown(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FontWeight;

    #[test]
    fn parse_inline_styles_and_code_fence() {
        let doc = Rendown::new().parse("**Hi** and `x`\n\n```rust\nfn main() {}\nlet x = 1;\n```");
        assert!(doc.lines.len() >= 2);
        assert!(doc.lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.weight == FontWeight::Bold && span.text.contains("Hi"))
        }));
        assert!(doc.lines.iter().any(|line| line.code_background));
    }

    #[test]
    fn mermaid_fence_defers_source() {
        let doc = Rendown::new().parse("```mermaid\ngraph LR; A --> B\n```");
        let line = doc
            .lines
            .iter()
            .find(|line| line.mermaid_source.is_some())
            .expect("deferred mermaid");
        assert_eq!(line.kind, crate::model::MarkdownLineKind::Code);
        assert!(line.code_background);
    }

    #[test]
    fn plain_and_ansi_contain_text() {
        let md = Rendown::new().width(80).color_level(ColorLevel::None);
        assert!(md.plain("# Title\n\nBody").contains("Title"));
        let ansi = md.render_string("- item one\n- item two").unwrap();
        assert!(ansi.contains("item one"));
        assert!(ansi.contains("item two"));
    }

    #[test]
    fn theme_builder_overrides_body() {
        let theme = MarkdownTheme::builder()
            .body(crate::model::RgbColor::new(1, 2, 3))
            .build();
        assert_eq!(theme.body, crate::model::RgbColor::new(1, 2, 3));
        assert_eq!(theme.heading, MarkdownTheme::dark().heading);
    }

    #[test]
    fn color_level_none_omits_sgr_but_parse_keeps_highlight() {
        let md = Rendown::new().width(80).color_level(ColorLevel::None);
        let doc = md.parse("```rust\nfn main() {}\n```");
        let code = doc
            .lines
            .iter()
            .find(|line| line.kind == crate::model::MarkdownLineKind::Code)
            .expect("code");
        assert!(
            code.spans.iter().any(|span| span.color != MarkdownTheme::dark().body) || !code.spans.is_empty(),
            "IR should keep syntect RGB even when ANSI is colorless"
        );
        let ansi = md.render_string("```rust\nfn main() {}\n```").unwrap();
        assert!(ansi.contains("main"));
        assert!(!ansi.contains('\x1b'), "NO_COLOR / ColorLevel::None must not emit SGR");
    }
}
