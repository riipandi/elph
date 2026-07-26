//! Diff viewer with syntax highlighting, hunk-aware rendering, and line numbers.
//!
//! ## Architecture
//!
//! This module is organised as a directory of focused submodules:
//!
//! | Submodule   | Responsibility                                         |
//! |-------------|--------------------------------------------------------|
//! | `types`     | Data model — [`DiffHunkLine`], [`DiffHunk`], [`DiffResult`] |
//! | `compute`   | Diff computation via `similar::TextDiff::grouped_ops`   |
//! | `highlight` | Syntax highlighting for diff lines (syntect)           |
//! | `render`    | Rendering helpers — colors, prefixes, hunk formatting   |
//! | (this file) | [`DiffMode`], [`DiffViewProps`], [`DiffView`] component  |
//!
//! ## Embedding in parent scroll regions
//!
//! Transcript tool cards live inside an outer [`ScrollView`]. Nesting another
//! scroll viewport (`ScrollBox`) collapses to a single garbled row under flex
//! layout. Set [`DiffViewProps::no_border`] (or `max_lines`) for **embedded**
//! mode: a plain column of lines that sizes to content and participates in the
//! parent scroller. Standalone demos keep a bordered [`ScrollBox`] with fixed
//! `height`.

pub mod compute;
pub mod highlight;
pub mod render;
pub mod types;

use iocraft::prelude::*;

use super::scroll_box::ScrollBox;
use super::theme::{UiTheme, resolve_ui_theme};

use compute::compute_diff;
use highlight::language_from_file_path;
pub use render::DiffLineNumberStyle;
use render::{render_unified_hunk, side_by_side_lines};

/// Default cap for embedded / transcript diff bodies (rows, including headers).
pub const EMBEDDED_DIFF_MAX_LINES: usize = 20;

// ── Diff display mode ──────────────────────────────────────────────────────

/// Diff display mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffMode {
    #[default]
    Unified,
    SideBySide,
}

// ── Layout measurement ─────────────────────────────────────────────────────

/// Count display rows for a unified diff — must match [`DiffView`] unified layout.
///
/// Use this for transcript row budgeting so outer scroll metrics match the painted tree.
///
/// - `show_file_header`: two `---/+++` lines when `true` and a path is known at render time
///   (pass `true` only when the view will also show headers).
/// - `show_hunk_header`: one `@@ … @@` line per hunk.
/// - `max_lines`: when set, rows are capped; one extra "… N more" line is counted if truncated.
pub fn unified_diff_display_rows(
    old_text: &str,
    new_text: &str,
    context_lines: usize,
    show_file_header: bool,
    show_hunk_header: bool,
    max_lines: Option<usize>,
) -> u16 {
    let context = if context_lines > 0 { context_lines } else { 3 };
    let result = compute_diff(old_text, new_text, context);

    let mut total: usize = 0;
    if show_file_header {
        total = total.saturating_add(2);
    }

    if result.hunks.is_empty() {
        total = total.saturating_add(1); // "(no changes)"
    } else {
        let n = result.hunks.len();
        for (i, hunk) in result.hunks.iter().enumerate() {
            if show_hunk_header {
                total = total.saturating_add(1);
            }
            total = total.saturating_add(hunk.lines.len());
            if i + 1 < n {
                total = total.saturating_add(1); // "···" gap
            }
        }
    }

    match max_lines {
        Some(max) if max > 0 && total > max => (max.saturating_add(1)).min(u16::MAX as usize) as u16,
        _ => total.min(u16::MAX as usize) as u16,
    }
}

// ── DiffView component ─────────────────────────────────────────────────────

/// Props for [`DiffView`].
#[derive(Clone, Default, Props)]
pub struct DiffViewProps {
    pub width: u16,
    /// Viewport height for standalone (scrollable) mode. Ignored when embedded
    /// (`no_border` or `max_lines` is set) — content height is used instead.
    pub height: u16,
    pub old_text: String,
    pub new_text: String,
    pub mode: DiffMode,
    pub side_by_side_min_width: u16,
    pub delete_color: Option<Color>,
    pub insert_color: Option<Color>,
    pub equal_color: Option<Color>,
    pub separator_color: Option<Color>,
    pub theme: Option<UiTheme>,

    /// File path for language detection and file header display.
    pub file_path: Option<String>,
    /// Enable syntax highlighting via syntect (requires `file_path` or explicit `language`).
    pub syntax_highlight: bool,
    /// Explicit language token (overrides `file_path` detection).
    pub language: Option<String>,
    /// Show `--- a/…` / `+++ b/…` file headers.
    pub show_file_header: bool,
    /// Show `@@ -old,count +new,count @@` hunk headers.
    pub show_hunk_header: bool,
    /// Line-number gutter style (default [`DiffLineNumberStyle::Single`]).
    pub line_numbers: DiffLineNumberStyle,
    /// Number of context lines per hunk (default: 3).
    pub context_lines: usize,
    /// Embed into a parent card/scroll region: no nested [`ScrollBox`], no border,
    /// content-sized height. Required for transcript tool cards.
    pub no_border: bool,
    /// Cap painted rows in embedded mode (default [`EMBEDDED_DIFF_MAX_LINES`] when
    /// embedded). Standalone scroll mode ignores this unless `no_border` is set.
    pub max_lines: Option<usize>,
}

/// Unified or side-by-side diff with optional syntax highlighting and line numbers.
///
/// Standalone: bordered [`ScrollBox`] with fixed `height`.
/// Embedded (`no_border`): plain column for parent scrollers (tool cards).
#[component]
pub fn DiffView(props: &DiffViewProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);

    // Resolve language for syntax highlighting
    let language: Option<String> = if props.syntax_highlight {
        props
            .language
            .clone()
            .or_else(|| language_from_file_path(props.file_path.as_deref()))
    } else {
        None
    };
    let language_ref: Option<&str> = language.as_deref();

    let context_lines = if props.context_lines > 0 {
        props.context_lines
    } else {
        3
    };

    // Nested ScrollView inside the transcript scroller collapses to one unreadable row.
    // Embedded mode: content-sized column that scrolls with the parent.
    let embedded = props.no_border || props.max_lines.is_some();
    let max_lines = if embedded {
        Some(props.max_lines.unwrap_or(EMBEDDED_DIFF_MAX_LINES).max(1))
    } else {
        None
    };

    let use_side_by_side = props.mode == DiffMode::SideBySide && props.width >= props.side_by_side_min_width.max(40);

    let mut children: Vec<AnyElement<'static>> = if use_side_by_side {
        let delete_color = props.delete_color.unwrap_or(theme.error);
        let insert_color = props.insert_color.unwrap_or(theme.success);
        let separator_color = props.separator_color.unwrap_or(theme.border);

        side_by_side_lines(
            &props.old_text,
            &props.new_text,
            props.width / 2,
            delete_color,
            insert_color,
            separator_color,
        )
    } else {
        // Unified mode with hunk-aware rendering
        let mut elements: Vec<AnyElement<'static>> = Vec::new();
        let w = props.width.max(1);

        // File header — full-width rows (never bare Text: ScrollView content is flex row).
        if props.show_file_header
            && let Some(path) = &props.file_path
        {
            for content in [format!("--- a/{}", path), format!("+++ b/{}", path)] {
                elements.push(
                    element! {
                        View(width: w, flex_direction: FlexDirection::Row, flex_shrink: 0f32) {
                            Text(content: content, color: theme.text_muted, wrap: TextWrap::NoWrap)
                        }
                    }
                    .into(),
                );
            }
        }

        // Compute hunks
        let result = compute_diff(&props.old_text, &props.new_text, context_lines);

        // Render each hunk
        let n_hunks = result.hunks.len();
        for (i, hunk) in result.hunks.iter().enumerate() {
            let hunk_elements = render_unified_hunk(
                hunk,
                language_ref,
                props.show_hunk_header,
                props.line_numbers,
                theme,
                props.width,
            );
            elements.extend(hunk_elements);

            // Gap between hunks (except after the last one)
            if i + 1 < n_hunks {
                elements.push(
                    element! {
                        View(width: w, flex_direction: FlexDirection::Row, flex_shrink: 0f32) {
                            Text(content: "···", color: theme.text_hint, wrap: TextWrap::NoWrap)
                        }
                    }
                    .into(),
                );
            }
        }

        // Fallback: if no hunks (identical content), show a single line
        if result.hunks.is_empty() {
            elements.push(
                element! {
                    View(width: w, flex_direction: FlexDirection::Row, flex_shrink: 0f32) {
                        Text(content: "(no changes)", color: theme.text_muted, wrap: TextWrap::NoWrap)
                    }
                }
                .into(),
            );
        }

        elements
    };

    // Cap rows in embedded mode so a huge file edit does not blow the transcript.
    if let Some(max) = max_lines
        && children.len() > max
    {
        let hidden = children.len().saturating_sub(max);
        let w = props.width.max(1);
        children.truncate(max);
        children.push(
            element! {
                View(width: w, flex_direction: FlexDirection::Row, flex_shrink: 0f32) {
                    Text(
                        content: format!("… {hidden} more lines"),
                        color: theme.text_hint,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            .into(),
        );
    }

    // Always stack lines in a column. ScrollView's content measurer uses default
    // flex row — bare multi-child lists concatenate into one horizontal strip.
    let column: AnyElement<'static> = element! {
        View(
            width: props.width.max(1),
            flex_direction: FlexDirection::Column,
            flex_shrink: 0f32,
            gap: 0,
        ) {
            #(children)
        }
    }
    .into();

    let root: AnyElement<'static> = if embedded {
        column
    } else {
        element! {
            ScrollBox(
                width: props.width,
                height: props.height,
                no_border: false,
                children: vec![column],
            )
        }
        .into()
    };
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_diff_display_rows_matches_simple_hunk() {
        // one delete + one insert + context_lines around — at least 2 change lines
        let rows = unified_diff_display_rows("a\nb\nc\n", "a\nx\nc\n", 1, false, true, None);
        // hunk header + equal a + delete b + insert x + equal c
        assert_eq!(rows, 5);
    }

    #[test]
    fn unified_diff_display_rows_empty_is_one() {
        assert_eq!(unified_diff_display_rows("same\n", "same\n", 3, false, true, None), 1);
    }

    #[test]
    fn unified_diff_display_rows_respects_max_lines() {
        // Every line changed → many hunk rows; cap must add the "… N more" row.
        let old = (0..40).map(|i| format!("old{i}\n")).collect::<String>();
        let new = (0..40).map(|i| format!("new{i}\n")).collect::<String>();
        let uncapped = unified_diff_display_rows(&old, &new, 0, false, true, None);
        assert!(uncapped > 5, "expected a large diff, got {uncapped}");
        let capped = unified_diff_display_rows(&old, &new, 0, false, true, Some(5));
        // 5 body + 1 "more" indicator
        assert_eq!(capped, 6);
    }
}
