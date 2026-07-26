//! Rendering helpers for unified and side-by-side diff display.

use iocraft::prelude::*;
use similar::ChangeTag;

use super::highlight::highlight_diff_line;
use super::types::DiffHunk;
use crate::components::theme::UiTheme;

// ── Per-tag styling ────────────────────────────────────────────────────────

/// Color for a single diff line based on its change tag.
pub fn diff_line_color(theme: UiTheme, tag: ChangeTag) -> Color {
    diff_line_color_with_overrides(theme, tag, None, None, None)
}

/// Color for a diff line with optional per-tag overrides.
pub fn diff_line_color_with_overrides(
    theme: UiTheme,
    tag: ChangeTag,
    delete_color: Option<Color>,
    insert_color: Option<Color>,
    equal_color: Option<Color>,
) -> Color {
    match tag {
        ChangeTag::Delete => delete_color.unwrap_or(theme.error),
        ChangeTag::Insert => insert_color.unwrap_or(theme.success),
        ChangeTag::Equal => equal_color.unwrap_or(theme.text_muted),
    }
}

/// Two-character prefix for a diff line (`"- "`, `"+ "`, or `"  "`).
pub fn diff_line_prefix(tag: ChangeTag) -> &'static str {
    match tag {
        ChangeTag::Delete => "- ",
        ChangeTag::Insert => "+ ",
        ChangeTag::Equal => "  ",
    }
}

/// Subtle background tint for changed lines.
pub fn diff_line_background(theme: UiTheme, tag: ChangeTag) -> Option<Color> {
    match tag {
        ChangeTag::Delete => Some(dim_color(theme.error, 0.15)),
        ChangeTag::Insert => Some(dim_color(theme.success, 0.15)),
        ChangeTag::Equal => None,
    }
}

/// Produce a dimmed version of a color by mixing with black (0.0 = full black).
fn dim_color(color: Color, factor: f64) -> Color {
    match color {
        Color::Rgb { r, g, b } => {
            let mix = |c: u8| (c as f64 * factor) as u8;
            Color::Rgb {
                r: mix(r),
                g: mix(g),
                b: mix(b),
            }
        }
        other => other,
    }
}

/// Build side-by-side diff lines.
pub fn side_by_side_lines(
    old_text: &str,
    new_text: &str,
    half_width: u16,
    delete_color: Color,
    insert_color: Color,
    separator_color: Color,
) -> Vec<AnyElement<'static>> {
    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();
    let rows = old_lines.len().max(new_lines.len()).max(1);

    (0..rows)
        .map(|i| {
            let left = old_lines.get(i).copied().unwrap_or("");
            let right = new_lines.get(i).copied().unwrap_or("");
            let left_trim = crate::utils::truncate_with_ellipsis(left, half_width as usize);
            let right_trim = crate::utils::truncate_with_ellipsis(right, half_width as usize);
            element! {
                View(width: half_width.saturating_mul(2), flex_direction: FlexDirection::Row) {
                    View(width: half_width) {
                        Text(content: left_trim, color: delete_color, wrap: TextWrap::NoWrap)
                    }
                    Text(content: " │ ", color: separator_color, wrap: TextWrap::NoWrap)
                    View(width: half_width) {
                        Text(content: right_trim, color: insert_color, wrap: TextWrap::NoWrap)
                    }
                }
            }
            .into()
        })
        .collect()
}

// ── Hunk header formatting ─────────────────────────────────────────────────

/// Render a hunk header line like `@@ -1,5 +1,6 @@`.
pub fn format_hunk_header(hunk: &DiffHunk) -> String {
    format!(
        "@@ -{},{} +{},{} @@",
        hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
    )
}

/// Render a file header line like `--- a/path` or `+++ b/path`.
pub fn format_file_header(side: &str, path: &str) -> String {
    format!("{} {}", side, path)
}

// ── Unified hunk rendering ─────────────────────────────────────────────────

/// Render a line number string for display.
fn lineno_str(lineno: Option<usize>) -> String {
    match lineno {
        Some(n) => format!("{:>4}", n),
        None => "    ".to_string(),
    }
}

/// Render one hunk as unified-diff iocraft elements (hunk header, lines).
///
/// Each line is optionally syntax-highlighted and line-numbered.
pub fn render_unified_hunk(
    hunk: &DiffHunk,
    language: Option<&str>,
    show_hunk_header: bool,
    show_line_numbers: bool,
    theme: UiTheme,
    width: u16,
) -> Vec<AnyElement<'static>> {
    let mut elements: Vec<AnyElement<'static>> =
        Vec::with_capacity(hunk.lines.len() + if show_hunk_header { 1 } else { 0 });

    // Hunk header
    if show_hunk_header {
        let header = format_hunk_header(hunk);
        elements.push(
            element! {
                Text(content: header, color: theme.accent, wrap: TextWrap::NoWrap)
            }
            .into(),
        );
    }

    // Line number column width
    let num_width: u16 = if show_line_numbers { 5 } else { 0 };

    // Content width = total width minus line number gutter
    let content_width = width.saturating_sub(num_width.saturating_add(1)).max(8) as usize;

    for line in &hunk.lines {
        let tag = line.tag;
        let num_str = if show_line_numbers {
            let old = lineno_str(line.old_lineno);
            let new = lineno_str(line.new_lineno);
            format!("{old} {new} ")
        } else {
            String::new()
        };

        // Strip trailing newline for display
        let display_text = line.text.trim_end_matches(['\r', '\n']);

        let prefix = diff_line_prefix(tag);
        let color = diff_line_color(theme, tag);
        let bg = diff_line_background(theme, tag);

        // For syntax-highlighted lines, use MixedText; otherwise plain Text.
        if language.is_some() && !display_text.is_empty() {
            let highlighted = highlight_diff_line(&format!("{prefix}{display_text}"), tag, language, theme);

            let mut row_children: Vec<AnyElement<'static>> =
                Vec::with_capacity(1 + if show_line_numbers { 1 } else { 0 });

            // Line numbers
            if show_line_numbers {
                row_children.push(
                    element! {
                        Text(content: num_str, color: theme.text_hint, wrap: TextWrap::NoWrap)
                    }
                    .into(),
                );
            }

            // Highlighted content
            row_children.push(
                element! {
                    MixedText(contents: highlighted, wrap: TextWrap::NoWrap)
                }
                .into(),
            );

            if let Some(bg_color) = bg {
                elements.push(
                    element! {
                        View(width: width, background_color: bg_color, flex_direction: FlexDirection::Row) {
                            #(row_children)
                        }
                    }
                    .into(),
                );
            } else {
                elements.push(
                    element! {
                        View(width: width, flex_direction: FlexDirection::Row) {
                            #(row_children)
                        }
                    }
                    .into(),
                );
            }
        } else {
            // Plain text (no syntax highlighting)
            let full_line = if show_line_numbers {
                format!("{num_str}{prefix}{display_text}")
            } else {
                format!("{prefix}{display_text}")
            };

            if display_text.is_empty() {
                elements.push(
                    element! {
                        Text(content: " ", color, wrap: TextWrap::NoWrap)
                    }
                    .into(),
                );
            } else if let Some(bg_color) = bg {
                elements.push(
                    element! {
                        View(width: width, background_color: bg_color) {
                            Text(
                                content: truncate_to_width(&full_line, content_width + num_width as usize + 2),
                                color,
                                wrap: TextWrap::NoWrap,
                            )
                        }
                    }
                    .into(),
                );
            } else {
                elements.push(
                    element! {
                        Text(
                            content: truncate_to_width(&full_line, content_width + num_width as usize + 2),
                            color,
                            wrap: TextWrap::NoWrap,
                        )
                    }
                    .into(),
                );
            }
        }
    }

    elements
}

/// Truncate a string to roughly `max_width` display columns.
fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    use crate::utils::display_width;
    if display_width(text) <= max_width {
        return text.to_string();
    }
    let target = max_width.saturating_sub(1);
    let mut out = String::new();
    let mut w = 0usize;
    for ch in text.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > target {
            out.push('…');
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::diff::compute::compute_diff;
    use crate::components::diff::types::DiffHunk;

    #[test]
    fn diff_line_prefix_covers_all_tags() {
        assert_eq!(diff_line_prefix(ChangeTag::Delete), "- ");
        assert_eq!(diff_line_prefix(ChangeTag::Insert), "+ ");
        assert_eq!(diff_line_prefix(ChangeTag::Equal), "  ");
    }

    #[test]
    fn diff_line_color_covers_all_tags() {
        let theme = UiTheme::default();
        assert_eq!(diff_line_color(theme, ChangeTag::Delete), theme.error);
        assert_eq!(diff_line_color(theme, ChangeTag::Insert), theme.success);
        assert_eq!(diff_line_color(theme, ChangeTag::Equal), theme.text_muted);
    }

    #[test]
    fn format_hunk_header_renders_correctly() {
        let hunk = DiffHunk {
            old_start: 1,
            old_count: 5,
            new_start: 1,
            new_count: 6,
            lines: vec![],
        };
        assert_eq!(format_hunk_header(&hunk), "@@ -1,5 +1,6 @@");
    }

    #[test]
    fn unified_diff_empty_inputs() {
        let result = compute_diff("", "", 3);
        assert!(result.hunks.is_empty());
    }

    #[test]
    fn unified_diff_non_empty() {
        let result = compute_diff("a\n", "b\n", 3);
        let theme = UiTheme::default();
        let elements = render_unified_hunk(&result.hunks[0], None, false, false, theme, 40);
        assert!(!elements.is_empty());
    }

    #[test]
    fn side_by_side_diff_handles_uneven_line_counts() {
        let theme = UiTheme::default();
        let lines = side_by_side_lines("one\ntwo", "alpha\nbeta\ngamma", 8, theme.error, theme.success, theme.border);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn format_file_header_side_and_path() {
        assert_eq!(format_file_header("--- a/", "main.rs"), "--- a/ main.rs");
    }

    #[test]
    fn diff_line_background_returns_some_for_changes() {
        let theme = UiTheme::default();
        assert!(diff_line_background(theme, ChangeTag::Delete).is_some());
        assert!(diff_line_background(theme, ChangeTag::Insert).is_some());
        assert!(diff_line_background(theme, ChangeTag::Equal).is_none());
    }

    #[test]
    fn render_unified_hunk_without_highlight_produces_elements() {
        let result = compute_diff("a\nb\nc\n", "a\nx\nc\n", 1);
        let theme = UiTheme::default();
        let elements = render_unified_hunk(&result.hunks[0], None, true, true, theme, 40);
        assert!(!elements.is_empty());
    }

    #[test]
    fn truncate_to_width_short_text() {
        assert_eq!(truncate_to_width("hello", 10), "hello");
    }

    #[test]
    fn truncate_to_width_long_text() {
        let result = truncate_to_width("a very long string that should be truncated", 20);
        assert!(result.len() < 50);
        assert!(result.contains('…') || result.len() <= 20);
    }

    #[test]
    fn truncate_to_width_zero() {
        assert_eq!(truncate_to_width("anything", 0), "");
    }
}
