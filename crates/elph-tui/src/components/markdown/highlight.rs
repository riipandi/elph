//! Fenced code block highlighting via syntect and `anstyle-syntect`.

use super::colors::syntect_to_styled_span;
use super::model::{MarkdownLine, MarkdownLineKind, StyledSpan};
use super::syntax::syntax_highlight_raw;
use super::theme::MarkdownTheme;

/// Highlight a fenced code block into per-line styled spans.
pub fn highlight_code_block(language: Option<&str>, code: &str, theme: &MarkdownTheme) -> Vec<MarkdownLine> {
    let fence_info = language.unwrap_or("");
    if let Some(highlighted) = syntax_highlight_raw(fence_info, code) {
        return highlighted
            .into_iter()
            .map(|regions| {
                let spans: Vec<StyledSpan> = regions
                    .into_iter()
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
                    code_background: true,
                }
            })
            .collect();
    }

    fallback_plain_code_block(code, theme)
}

fn fallback_plain_code_block(code: &str, theme: &MarkdownTheme) -> Vec<MarkdownLine> {
    let mut lines = Vec::new();
    for line in code.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        lines.push(MarkdownLine {
            kind: MarkdownLineKind::Code,
            spans: vec![StyledSpan::plain(trimmed, theme.body)],
            code_background: true,
        });
    }
    if lines.is_empty() {
        lines.push(MarkdownLine {
            kind: MarkdownLineKind::Code,
            spans: vec![StyledSpan::plain("", theme.body)],
            code_background: true,
        });
    }
    lines
}
