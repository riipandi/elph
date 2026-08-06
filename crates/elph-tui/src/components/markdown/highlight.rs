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
/// so that what you see is exactly what was measured.
///
/// Strategy (the output **never exceeds** `max_width`, so it never overflows the terminal):
/// 1. **Strict Unicode** — compact to fit `max_width`. If it fits, great.
/// 2. **Strict ASCII** — denser glyphs may fit where Unicode can't.
/// 3. **Soft Unicode/ASCII** — best-effort compaction, then any over-wide lines are truncated
///    with an ellipsis so the diagram stays inside the column budget.
/// Only a genuinely *invalid* mermaid source returns an error.
pub fn render_mermaid_at_width(source: &str, max_width: u16) -> Result<String, mermaid_text::Error> {
    let max_width = max_width.max(1) as usize;

    // 1. Strict Unicode — compact to fit.
    let strict_unicode = mermaid_text::RenderOptions {
        max_width: Some(max_width),
        max_width_strict: true,
        ascii: false,
        color: false,
        ..Default::default()
    };
    if let Ok(output) = mermaid_text::render_with_options(source, &strict_unicode) {
        return Ok(output);
    }

    // 2. Strict ASCII — denser glyphs may fit where Unicode can't.
    let strict_ascii = mermaid_text::RenderOptions {
        max_width: Some(max_width),
        max_width_strict: true,
        ascii: true,
        color: false,
        ..Default::default()
    };
    if let Ok(output) = mermaid_text::render_with_options(source, &strict_ascii) {
        return Ok(output);
    }

    // 3. Soft budget (non-strict) — render at whatever width the layout produces, then
    // truncate any over-wide lines so the diagram can never overflow the terminal.
    // This keeps a valid diagram as a diagram (never a raw code block) while guaranteeing
    // it fits inside the available columns.
    let soft_unicode = mermaid_text::RenderOptions {
        max_width: Some(max_width),
        max_width_strict: false,
        ascii: false,
        color: false,
        ..Default::default()
    };
    if let Ok(output) = mermaid_text::render_with_options(source, &soft_unicode) {
        return Ok(truncate_diagram_lines(&output, max_width));
    }

    let soft_ascii = mermaid_text::RenderOptions {
        max_width: Some(max_width),
        max_width_strict: false,
        ascii: true,
        color: false,
        ..Default::default()
    };
    mermaid_text::render_with_options(source, &soft_ascii).map(|output| truncate_diagram_lines(&output, max_width))
}

/// Truncate each diagram line so no line exceeds `max_width` display columns.
/// Uses the same ellipsis style as table-cell truncation for a consistent look.
fn truncate_diagram_lines(output: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    output
        .lines()
        .map(|line| {
            if line.width() as usize > max_width {
                crate::utils::truncate_with_ellipsis(line, max_width)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    fn mermaid_render_never_reverts_on_overflow() {
        // A diagram that overflows at a very narrow width must STILL render as a diagram
        // (truncated), never reverting to raw source and never overflowing the budget.
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy] --> D[Verify] --> E[Release]";
        let output = render_mermaid_at_width(src, 20).expect("valid diagram must render");
        // Every line must fit within the width budget (20 cols), so no terminal overflow.
        for line in output.lines() {
            assert!(
                line.chars().count() <= 20,
                "line exceeds width: {line:?} ({} chars)",
                line.chars().count()
            );
        }
        // The diagram retains its structure (box-drawing characters present).
        assert!(output.contains('─') || output.contains('-'), "diagram keeps its edges");
    }

    #[test]
    fn mermaid_render_compacts_to_fit_wide_diagram() {
        // A diagram that CAN fit should be compacted (strict width path succeeds).
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy]";
        let output = render_mermaid_at_width(src, 80).expect("fits within 80 cols");
        assert!(output.contains("Build"));
        assert!(output.contains("Deploy"));
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
    fn mermaid_render_truncates_overflowing_lines() {
        // Even if the layout can't compact enough, individual lines are truncated so the
        // diagram never exceeds the column budget.
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy] --> D[Verify] --> E[Release]";
        let output = render_mermaid_at_width(src, 16).expect("renders");
        for line in output.lines() {
            assert!(line.chars().count() <= 16, "overflowing line not truncated: {line:?}");
        }
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
