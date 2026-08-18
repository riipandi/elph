//! Fenced code block highlighting via syntect.

use crate::blocks::code_block_uses_card_background;
use crate::model::{MarkdownLine, MarkdownLineKind, StyledSpan};
use crate::theme::MarkdownTheme;

/// Highlight a fenced code block into per-line styled spans.
///
/// Mermaid fences always produce a single deferred line (`mermaid_source` set).
/// Diagram rendering happens later (ANSI / TUI) so width is known.
pub fn highlight_code_block(language: Option<&str>, code: &str, theme: &MarkdownTheme) -> Vec<MarkdownLine> {
    let use_card = code_block_uses_card_background(code);

    if language.is_some_and(is_mermaid_language) {
        return vec![MarkdownLine {
            kind: MarkdownLineKind::Code,
            spans: vec![StyledSpan::plain("", theme.body)],
            code_background: true,
            table: None,
            mermaid_source: Some(code.to_string()),
        }];
    }

    #[cfg(feature = "highlight")]
    {
        use crate::colors::syntect_to_styled_span;
        use crate::syntax::syntax_highlight_raw;

        let fence_info = language.unwrap_or("");
        if let Some(highlighted) = syntax_highlight_raw(fence_info, code) {
            return highlighted
                .into_iter()
                .map(|regions| {
                    let spans: Vec<StyledSpan> = regions
                        .into_iter()
                        .map(|(style, text)| (style, text.trim_end_matches(['\n', '\r']).to_string()))
                        .filter(|(_, text)| !text.is_empty())
                        .map(|(style, text)| syntect_to_styled_span(style, text, theme.body))
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
                        mermaid_source: None,
                    }
                })
                .collect();
        }
        if !fence_info.is_empty() {
            log::debug!("syntax highlight fallback to plain lang={fence_info}");
        }
    }

    #[cfg(not(feature = "highlight"))]
    let _ = language;
    fallback_plain_code_block(code, theme, use_card)
}

fn is_mermaid_language(lang: &str) -> bool {
    let lang = lang.trim();
    lang.eq_ignore_ascii_case("mermaid") || lang.to_ascii_lowercase().starts_with("mermaid")
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
            mermaid_source: None,
        });
    }
    if lines.is_empty() {
        lines.push(MarkdownLine {
            kind: MarkdownLineKind::Code,
            spans: vec![StyledSpan::plain("", theme.body)],
            code_background: use_card,
            table: None,
            mermaid_source: None,
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn mermaid_stores_deferred_source() {
        let src = "graph LR; A[Build] --> B[Deploy]";
        let lines = highlight_code_block(Some("mermaid"), src, &MarkdownTheme::default());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].code_background);
        assert_eq!(lines[0].kind, MarkdownLineKind::Code);
        assert_eq!(lines[0].mermaid_source.as_deref(), Some(src));
    }

    #[test]
    fn mermaid_language_trims() {
        let src = "graph LR; A --> B";
        let lines = highlight_code_block(Some("mermaid "), src, &MarkdownTheme::default());
        assert_eq!(lines[0].mermaid_source.as_deref(), Some(src));
        let mixed = highlight_code_block(Some("Mermaid"), src, &MarkdownTheme::default());
        assert_eq!(mixed[0].mermaid_source.as_deref(), Some(src));
    }

    #[cfg(feature = "highlight")]
    #[test]
    fn non_mermaid_language_uses_syntect_path() {
        let src = "let x = 1;";
        let lines = highlight_code_block(Some("rust"), src, &MarkdownTheme::default());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].mermaid_source.is_none());
        assert!(!lines[0].spans.is_empty());
    }
}
