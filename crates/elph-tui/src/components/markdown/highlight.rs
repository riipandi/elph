//! Fenced code block highlighting via syntect and `anstyle-syntect`.

use super::blocks::code_block_uses_card_background;
use super::colors::syntect_to_styled_span;
use super::model::{MarkdownLine, MarkdownLineKind, StyledSpan};
use super::syntax::syntax_highlight_raw;
use super::theme::MarkdownTheme;

/// Highlight a fenced code block into per-line styled spans.
pub fn highlight_code_block(language: Option<&str>, code: &str, theme: &MarkdownTheme) -> Vec<MarkdownLine> {
    let use_card = code_block_uses_card_background(code);
    let fence_info = language.unwrap_or("");
    if let Some(highlighted) = syntax_highlight_raw(fence_info, code) {
        return highlighted
            .into_iter()
            .map(|regions| {
                // syntect emits line text with a trailing newline (LinesWithEndings); strip it
                // so wrapped row counts and MixedText layout stay correct.
                let spans: Vec<StyledSpan> = regions
                    .into_iter()
                    .map(|(style, text)| (style, text.trim_end_matches(['\n', '\r']).to_string()))
                    .filter(|(_, text)| !text.is_empty())
                    .map(|(style, text)| syntect_to_styled_span(style, text, theme.body, theme.ui))
                    .collect();
                MarkdownLine {
                    kind: MarkdownLineKind::Code,
                    spans: if spans.is_empty() {
                        vec![StyledSpan::plain("", theme.body)]
                    } else {
                        spans
                    },
                    code_background: use_card,
                    table: None,
                }
            })
            .collect();
    }

    fallback_plain_code_block(code, theme, use_card)
}

fn fallback_plain_code_block(code: &str, theme: &MarkdownTheme, use_card: bool) -> Vec<MarkdownLine> {
    let mut lines = Vec::new();
    for line in code.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        lines.push(MarkdownLine {
            kind: MarkdownLineKind::Code,
            spans: vec![StyledSpan::plain(trimmed, theme.body)],
            code_background: use_card,
            table: None,
        });
    }
    if lines.is_empty() {
        lines.push(MarkdownLine {
            kind: MarkdownLineKind::Code,
            spans: vec![StyledSpan::plain("", theme.body)],
            code_background: use_card,
            table: None,
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::markdown::MarkdownTheme;

    #[test]
    fn single_line_code_block_skips_card_background() {
        let lines = highlight_code_block(Some("rust"), "let x = 1;", &MarkdownTheme::default());
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].code_background);
    }

    #[test]
    fn multi_line_code_block_uses_card_background() {
        let lines = highlight_code_block(Some("rust"), "let a = 1;\nlet b = 2;\n", &MarkdownTheme::default());
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.code_background));
    }
}
