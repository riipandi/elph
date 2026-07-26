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
//! The old helper functions (`unified_lines`, `side_by_side_lines`,
//! `diff_line_color`, `diff_line_prefix`) are re-exported here for
//! backward compatibility.

pub mod compute;
pub mod highlight;
pub mod render;
pub mod types;

use iocraft::prelude::*;

use super::scroll_box::ScrollBox;
use super::theme::{UiTheme, resolve_ui_theme};

// ── Re-exports for backward compatibility ──────────────────────────────────
//
// External test files and callers import these from `elph_tui::components::diff::*`
// rather than from the submodule paths.  The `pub use` also brings each name
// into scope inside this module, so no separate `use` is needed.
pub use compute::{compute_diff, diff_result_with_paths};
pub use highlight::{highlight_diff_line, language_from_file_path, language_from_shebang};
pub use render::{
    diff_line_background, diff_line_color, diff_line_color_with_overrides, diff_line_prefix, format_file_header,
    format_hunk_header, render_unified_hunk, side_by_side_lines, unified_lines,
};
pub use types::{DiffHunk, DiffHunkLine, DiffResult};

// ── Diff display mode ──────────────────────────────────────────────────────

/// Diff display mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffMode {
    #[default]
    Unified,
    SideBySide,
}

// ── DiffView component ─────────────────────────────────────────────────────

/// Props for [`DiffView`].
#[derive(Clone, Default, Props)]
pub struct DiffViewProps {
    pub width: u16,
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

    // ── New optional props (all backward-compatible) ──
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
    /// Show old/new line numbers in the gutter.
    pub show_line_numbers: bool,
    /// Number of context lines per hunk (default: 3).
    pub context_lines: usize,
}

/// Scrollable unified or side-by-side diff with optional syntax highlighting
/// and line numbers.
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

    let use_side_by_side = props.mode == DiffMode::SideBySide && props.width >= props.side_by_side_min_width.max(40);

    let children: Vec<AnyElement<'static>> = if use_side_by_side {
        // Side-by-side mode (uses the legacy approach for now)
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

        // File header
        if props.show_file_header
            && let Some(path) = &props.file_path
        {
            elements.push(
                element! {
                    Text(
                        content: format!("--- a/{}", path),
                        color: theme.text_muted,
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            );
            elements.push(
                element! {
                    Text(
                        content: format!("+++ b/{}", path),
                        color: theme.text_muted,
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            );
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
                props.show_line_numbers,
                theme,
                props.width,
            );
            elements.extend(hunk_elements);

            // Gap between hunks (except after the last one)
            if i + 1 < n_hunks {
                elements.push(
                    element! {
                        Text(content: "···", color: theme.text_hint, wrap: TextWrap::NoWrap)
                    }
                    .into(),
                );
            }
        }

        // Fallback: if no hunks (identical content), show a single line
        if result.hunks.is_empty() {
            elements.push(
                element! {
                    Text(content: "(no changes)", color: theme.text_muted, wrap: TextWrap::NoWrap)
                }
                .into(),
            );
        }

        elements
    };

    element! {
        ScrollBox(
            width: props.width,
            height: props.height,
            children: children,
        )
    }
}
