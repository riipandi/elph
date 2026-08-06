//! Fenced code block highlighting via syntect and `anstyle-syntect`.

use super::blocks::code_block_uses_card_background;
use super::colors::syntect_to_styled_span;
use super::model::{MarkdownLine, MarkdownLineKind, StyledSpan};
use super::syntax::syntax_highlight_raw;
use super::theme::MarkdownTheme;

/// Highlight a fenced code block into per-line styled spans.
pub fn highlight_code_block(language: Option<&str>, code: &str, theme: &MarkdownTheme) -> Vec<MarkdownLine> {
    let use_card = code_block_uses_card_background(code);

    // Intercept mermaid diagrams — defer rendering to paint/measure time so the diagram
    // can be sized to the actual terminal width. Store raw source; render lazily.
    if language.is_some_and(|lang| lang.trim() == "mermaid") {
        return vec![MarkdownLine {
            kind: MarkdownLineKind::Code,
            spans: vec![StyledSpan::plain("", theme.body)],
            // Mermaid diagrams are multi-line — always use the tinted card background.
            code_background: true,
            table: None,
            mermaid_source: Some(code.to_string()),
        }];
    }

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
                    mermaid_source: None,
                }
            })
            .collect();
    }

    fallback_plain_code_block(code, theme, use_card)
}

/// Render a mermaid diagram to Unicode box-drawing text, compacting to fit `max_width`.
///
/// Shared by both the paint path ([`super::render`]) and the measure path ([`super::layout`])
/// so that what you see is exactly what was measured. Uses `max_width_strict` so that a diagram
/// that cannot fit even at minimum gaps returns [`mermaid_text::Error::TooWide`] instead of an
/// over-wide string — letting the caller fall back to raw source knowingly.
///
/// Strategy: try Unicode first; if it overflows, try ASCII (denser); if that also fails, return
/// the error so the caller can fall back to the raw source text.
pub fn render_mermaid_at_width(source: &str, max_width: u16) -> Result<String, mermaid_text::Error> {
    let options = mermaid_text::RenderOptions {
        max_width: Some(max_width as usize),
        max_width_strict: true,
        ascii: false,
        color: false,
        ..Default::default()
    };
    match mermaid_text::render_with_options(source, &options) {
        Ok(output) => Ok(output),
        Err(err) => {
            // Unicode didn't fit — try ASCII which is denser and may fit where Unicode can't.
            let ascii_options = mermaid_text::RenderOptions {
                max_width: Some(max_width as usize),
                max_width_strict: true,
                ascii: true,
                color: false,
                ..Default::default()
            };
            mermaid_text::render_with_options(source, &ascii_options).map_err(|_| err)
        }
    }
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

    #[test]
    fn mermaid_stores_deferred_source() {
        // Deferred rendering: parse-time stores raw source, produces a single code line.
        let src = "graph LR; A[Build] --> B[Deploy]";
        let lines = highlight_code_block(Some("mermaid"), src, &MarkdownTheme::default());
        assert_eq!(lines.len(), 1, "deferred mermaid produces exactly one line");
        assert!(lines[0].code_background, "mermaid always uses card background");
        assert_eq!(lines[0].kind, MarkdownLineKind::Code);
        assert_eq!(lines[0].mermaid_source.as_deref(), Some(src));
    }

    #[test]
    fn mermaid_render_at_width_produces_diagram() {
        // The shared render function produces a diagram with the node labels.
        let src = "graph LR; A[Build] --> B[Deploy]";
        let output = render_mermaid_at_width(src, 120).expect("valid mermaid renders");
        assert!(output.contains("Build"), "diagram contains 'Build'");
        assert!(output.contains("Deploy"), "diagram contains 'Deploy'");
    }

    #[test]
    fn mermaid_render_falls_back_to_ascii_on_overflow() {
        // A diagram that overflows at a given width: if it can't fit even at minimum gaps
        // (in both Unicode and ASCII), it returns Err so the caller can fall back to raw source.
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy] --> D[Verify] --> E[Release]";
        let result = render_mermaid_at_width(src, 20);
        // At width 20 this chain cannot fit even in ASCII — expect Err, not panic.
        assert!(
            result.is_err(),
            "diagram that cannot fit returns Err (caller falls back to raw source)"
        );
    }

    #[test]
    fn mermaid_render_strict_width_rejects_overflow() {
        // Strict mode: a diagram that exceeds the budget returns TooWide, not an over-wide string.
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy]";
        // At width 10 the diagram definitely cannot fit.
        let result = render_mermaid_at_width(src, 10);
        assert!(result.is_err(), "strict width rejects overflow");
    }

    #[test]
    fn mermaid_render_succeeds_within_budget() {
        // A diagram that fits within the budget renders successfully.
        let src = "graph LR; A[Build] --> B[Deploy]";
        let output = render_mermaid_at_width(src, 80).expect("fits within 80 cols");
        assert!(output.contains("Build"));
        assert!(output.contains("Deploy"));
    }

    #[test]
    fn mermaid_render_returns_error_for_invalid_source() {
        // Invalid mermaid source should return an error so caller can fall back to raw source.
        let src = "this is not valid mermaid {{{";
        let result = render_mermaid_at_width(src, 80);
        assert!(result.is_err(), "invalid mermaid returns Err so caller can fallback");
    }

    #[test]
    fn mermaid_language_must_be_exact() {
        // "mermaid " with trailing space — trim() handles it.
        let src = "graph LR; A --> B";
        let lines = highlight_code_block(Some("mermaid "), src, &MarkdownTheme::default());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].mermaid_source.as_deref(), Some(src));
    }

    #[test]
    fn non_mermaid_language_uses_syntect_path() {
        let src = "let x = 1;";
        let lines = highlight_code_block(Some("rust"), src, &MarkdownTheme::default());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].mermaid_source.is_none(), "non-mermaid has no deferred source");
        // Rust syntax highlighting should produce styled spans (not plain fallback).
        assert!(!lines[0].spans.is_empty());
    }
}
