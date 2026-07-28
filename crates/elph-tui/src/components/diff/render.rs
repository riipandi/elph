//! Rendering helpers for unified and side-by-side diff display.

use iocraft::prelude::*;
use similar::ChangeTag;

use super::highlight::highlight_diff_line;
use super::types::DiffHunk;
use crate::components::theme::UiTheme;

// ── Line number gutter style ───────────────────────────────────────────────

/// How line numbers are shown in the unified-diff gutter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffLineNumberStyle {
    /// No line-number gutter.
    None,
    /// One column (default): old # for deletes, new # for inserts/context.
    #[default]
    Single,
    /// Two columns: old and new side-by-side (`   5    6 `).
    Dual,
}

impl DiffLineNumberStyle {
    /// Display width of the gutter including trailing space (0 when [`None`](Self::None)).
    pub fn gutter_width(self) -> u16 {
        match self {
            Self::None => 0,
            Self::Single => 5, // `1234 `
            Self::Dual => 10,  // `1234 5678 `
        }
    }

    /// Format the gutter text for one hunk line.
    pub fn format(self, tag: ChangeTag, old_lineno: Option<usize>, new_lineno: Option<usize>) -> String {
        match self {
            Self::None => String::new(),
            Self::Single => {
                let n = match tag {
                    ChangeTag::Delete => old_lineno,
                    ChangeTag::Insert => new_lineno,
                    ChangeTag::Equal => new_lineno.or(old_lineno),
                };
                match n {
                    Some(n) => format!("{n:>4} "),
                    None => "     ".to_string(),
                }
            }
            Self::Dual => {
                format!("{} {} ", lineno_str(old_lineno), lineno_str(new_lineno))
            }
        }
    }
}

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

/// Background tint for changed lines (delete = red wash, insert = green wash).
///
/// Factor is tuned for dark terminals so status remains readable without washing out text.
pub fn diff_line_background(theme: UiTheme, tag: ChangeTag) -> Option<Color> {
    match tag {
        ChangeTag::Delete => Some(dim_color(theme.error, 0.28)),
        ChangeTag::Insert => Some(dim_color(theme.success, 0.28)),
        ChangeTag::Equal => None,
    }
}

/// Gutter (line-number) foreground: status-tinted for changes, muted for context.
pub fn diff_lineno_color(theme: UiTheme, tag: ChangeTag) -> Color {
    match tag {
        ChangeTag::Delete => theme.error,
        ChangeTag::Insert => theme.success,
        ChangeTag::Equal => theme.text_hint,
    }
}

/// Produce a dimmed version of a color by mixing with black (0.0 = full black).
fn dim_color(color: Color, factor: f64) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    match color {
        Color::Rgb { r, g, b } => {
            let mix = |c: u8| ((c as f64) * factor).round() as u8;
            Color::Rgb {
                r: mix(r),
                g: mix(g),
                b: mix(b),
            }
        }
        // Named ANSI fallbacks so non-RGB themes still get a wash.
        Color::Red | Color::DarkRed => Color::Rgb {
            r: (180.0 * factor) as u8,
            g: (30.0 * factor) as u8,
            b: (30.0 * factor) as u8,
        },
        Color::Green | Color::DarkGreen => Color::Rgb {
            r: (30.0 * factor) as u8,
            g: (140.0 * factor) as u8,
            b: (40.0 * factor) as u8,
        },
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

/// Render a line number string for dual-column display (fixed width 4, blank if absent).
fn lineno_str(lineno: Option<usize>) -> String {
    match lineno {
        Some(n) => format!("{n:>4}"),
        None => "    ".to_string(),
    }
}

/// One full-width row so parent flex row (e.g. ScrollView content) cannot
/// concatenate lines side-by-side into a single unreadable strip.
pub(crate) fn diff_line_row(width: u16, bg: Option<Color>, children: Vec<AnyElement<'static>>) -> AnyElement<'static> {
    let w = width.max(1);
    if let Some(bg_color) = bg {
        element! {
            View(
                width: w,
                flex_direction: FlexDirection::Row,
                flex_shrink: 0f32,
                background_color: bg_color,
                overflow: Overflow::Hidden,
            ) {
                #(children)
            }
        }
        .into()
    } else {
        element! {
            View(
                width: w,
                flex_direction: FlexDirection::Row,
                flex_shrink: 0f32,
                overflow: Overflow::Hidden,
            ) {
                #(children)
            }
        }
        .into()
    }
}

/// Render one hunk as unified-diff iocraft elements (hunk header, lines).
///
/// Each line is a full-width row with optional status background and line numbers.
pub fn render_unified_hunk(
    hunk: &DiffHunk,
    language: Option<&str>,
    show_hunk_header: bool,
    line_numbers: DiffLineNumberStyle,
    theme: UiTheme,
    width: u16,
) -> Vec<AnyElement<'static>> {
    let mut elements: Vec<AnyElement<'static>> =
        Vec::with_capacity(hunk.lines.len() + if show_hunk_header { 1 } else { 0 });

    // Hunk header — truncate to row width so long file paths cannot overflow.
    if show_hunk_header {
        let header = format_hunk_header(hunk);
        let header_text = truncate_to_width(&header, width.max(1) as usize);
        elements.push(diff_line_row(
            width,
            None,
            vec![
                element! {
                    Text(content: header_text, color: theme.accent, wrap: TextWrap::NoWrap)
                }
                .into(),
            ],
        ));
    }

    let num_width = line_numbers.gutter_width();
    // Content width = total width minus gutter and status prefix.
    // Clamp to at least 1 so we never exceed the available row width (the old
    // `.max(8)` could make content wider than the row on narrow terminals).
    let content_width = width.saturating_sub(num_width.saturating_add(2)).max(1) as usize;

    for line in &hunk.lines {
        let tag = line.tag;
        let num_str = line_numbers.format(tag, line.old_lineno, line.new_lineno);

        // Strip trailing newline for display
        let display_text = line.text.trim_end_matches(['\r', '\n']);

        let prefix = diff_line_prefix(tag);
        let color = diff_line_color(theme, tag);
        let bg = diff_line_background(theme, tag);
        let lineno_color = diff_lineno_color(theme, tag);

        // For syntax-highlighted lines, use MixedText; otherwise plain Text.
        if language.is_some() && !display_text.is_empty() {
            // Truncate content before highlighting so highlighted output fits within
            // the available width (content_width accounts for gutter + prefix).
            let truncated = truncate_to_width(display_text, content_width);
            let highlighted = highlight_diff_line(&format!("{prefix}{truncated}"), tag, language, theme);

            let mut row_children: Vec<AnyElement<'static>> = Vec::with_capacity(
                1 + if line_numbers != DiffLineNumberStyle::None {
                    1
                } else {
                    0
                },
            );

            if line_numbers != DiffLineNumberStyle::None {
                row_children.push(
                    element! {
                        Text(content: num_str, color: lineno_color, wrap: TextWrap::NoWrap)
                    }
                    .into(),
                );
            }

            row_children.push(
                element! {
                    MixedText(contents: highlighted, wrap: TextWrap::NoWrap)
                }
                .into(),
            );

            elements.push(diff_line_row(width, bg, row_children));
        } else {
            // Plain text (no syntax highlighting): gutter + status-colored body.
            let body = if display_text.is_empty() {
                format!("{prefix} ")
            } else {
                format!("{prefix}{}", truncate_to_width(display_text, content_width))
            };

            let mut row_children: Vec<AnyElement<'static>> = Vec::with_capacity(
                1 + if line_numbers != DiffLineNumberStyle::None {
                    1
                } else {
                    0
                },
            );
            if line_numbers != DiffLineNumberStyle::None {
                row_children.push(
                    element! {
                        Text(content: num_str, color: lineno_color, wrap: TextWrap::NoWrap)
                    }
                    .into(),
                );
            }
            row_children.push(
                element! {
                    Text(content: body, color, wrap: TextWrap::NoWrap)
                }
                .into(),
            );
            elements.push(diff_line_row(width, bg, row_children));
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
        let elements = render_unified_hunk(&result.hunks[0], None, false, DiffLineNumberStyle::None, theme, 40);
        assert!(!elements.is_empty());
    }

    #[test]
    fn single_column_line_numbers_pick_side_by_tag() {
        assert_eq!(DiffLineNumberStyle::Single.format(ChangeTag::Delete, Some(12), None), "  12 ");
        assert_eq!(DiffLineNumberStyle::Single.format(ChangeTag::Insert, None, Some(13)), "  13 ");
        assert_eq!(DiffLineNumberStyle::Single.format(ChangeTag::Equal, Some(1), Some(1)), "   1 ");
        assert_eq!(DiffLineNumberStyle::Single.gutter_width(), 5);
        assert_eq!(DiffLineNumberStyle::Dual.gutter_width(), 10);
        assert_eq!(
            DiffLineNumberStyle::Dual.format(ChangeTag::Equal, Some(3), Some(4)),
            "   3    4 "
        );
    }

    #[test]
    fn diff_line_background_tints_changes() {
        let theme = UiTheme::dark();
        assert!(diff_line_background(theme, ChangeTag::Delete).is_some());
        assert!(diff_line_background(theme, ChangeTag::Insert).is_some());
        assert!(diff_line_background(theme, ChangeTag::Equal).is_none());
        assert_eq!(diff_lineno_color(theme, ChangeTag::Delete), theme.error);
        assert_eq!(diff_lineno_color(theme, ChangeTag::Insert), theme.success);
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
        let elements = render_unified_hunk(&result.hunks[0], None, true, DiffLineNumberStyle::Single, theme, 40);
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

    #[test]
    fn highlighted_diff_line_truncates_long_content() {
        // Verify that highlighted diff lines truncate to content_width, not overflow.
        let long_line = "a".repeat(200);
        let old_text = format!("{long_line}\n");
        let new_text = "x\n".to_string();
        let result = compute_diff(&old_text, &new_text, 3);
        let theme = UiTheme::default();
        let width = 40u16;
        // With Single line numbers (gutter=5) + prefix(2) = 7 chars reserved.
        // content_width = 40 - 5 - 2 = 33. Long line should be truncated.
        let elements =
            render_unified_hunk(&result.hunks[0], Some("rust"), false, DiffLineNumberStyle::Single, theme, width);
        assert!(!elements.is_empty());
        // The rendered output should fit within the width (no overflow).
        let rendered = element! { View(width: width) { #(elements) } }.to_string();
        // Truncated line should contain ellipsis or fit within width.
        assert!(
            rendered.contains('…') || rendered.lines().all(|line| line.chars().count() <= width as usize),
            "diff line should be truncated to fit width {width}"
        );
    }
}
