//! Layout tokens and pure helpers for dialog shells.

use crate::components::select::{SELECT_LIST_AUTO_HEIGHT, select_list_total_rows};
use crate::components::theme::UiTheme;
use crate::types::{DialogTodoItem, SelectOption};
use crate::wrapped_transcript_row_count;
use iocraft::prelude::Color;

/// Visual tokens for [`super::DialogShell`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogChrome {
    pub width: u16,
    pub min_content_height: u16,
    /// Vertical inset between the round border and header/body content.
    pub padding_vertical: u16,
    /// Horizontal inset between the round border and header/body content.
    pub padding_horizontal: u16,
    /// Space below the header row before the divider.
    pub header_gap: u16,
    /// Space between major body sections (prompt, list, actions).
    pub body_gap: u16,
    /// Space between stacked rows inside list-style bodies.
    pub row_gap: u16,
    pub border_color: Color,
    pub title_color: Color,
    pub muted_color: Color,
    pub background: Color,
    pub esc_hint: String,
    pub show_divider: bool,
    /// Minimal vertical padding, no header divider — for read-only viewers (e.g. system prompt).
    pub slim_header: bool,
}

impl Default for DialogChrome {
    fn default() -> Self {
        Self::from_theme(UiTheme::default(), 52)
    }
}

impl DialogChrome {
    /// Build dialog chrome from shared theme tokens and outer width.
    pub fn from_theme(theme: UiTheme, width: u16) -> Self {
        Self {
            width: width.max(20),
            min_content_height: 6,
            padding_vertical: theme.dialog_shell_inset_vertical(),
            padding_horizontal: theme.dialog_shell_inset_horizontal(),
            header_gap: theme.dialog_header_gap(),
            body_gap: theme.dialog_section_gap(),
            row_gap: theme.dialog_row_gap(),
            border_color: theme.shell_zone_border_color(true),
            title_color: theme.text_primary,
            muted_color: theme.text_muted,
            background: Color::Reset,
            esc_hint: "[esc]".to_string(),
            show_divider: true,
            slim_header: false,
        }
    }

    pub fn with_width(mut self, width: u16) -> Self {
        self.width = width.max(20);
        self
    }

    pub fn with_theme(mut self, theme: UiTheme) -> Self {
        if self.slim_header {
            self.padding_vertical = 0;
            self.header_gap = 0;
            self.body_gap = 0;
            self.show_divider = false;
        } else {
            self.padding_vertical = theme.dialog_shell_inset_vertical();
            self.header_gap = theme.dialog_header_gap();
            self.body_gap = theme.dialog_section_gap();
            self.show_divider = true;
        }
        self.padding_horizontal = theme.dialog_shell_inset_horizontal();
        self.row_gap = theme.dialog_row_gap();
        self.border_color = theme.shell_zone_border_color(true);
        self.title_color = theme.text_primary;
        self.muted_color = theme.text_muted;
        self.background = Color::Reset;
        self
    }

    pub fn with_slim_header(mut self, slim: bool) -> Self {
        self.slim_header = slim;
        self
    }

    pub fn content_width(&self) -> u16 {
        self.width
            .saturating_sub(2)
            .saturating_sub(self.padding_horizontal.saturating_mul(2))
    }

    pub fn inner_body_width(&self) -> u16 {
        self.content_width()
    }
}

/// Rows consumed by the header row, divider, and their trailing gaps inside the shell.
pub fn dialog_shell_chrome_rows(chrome: &DialogChrome) -> u16 {
    let mut rows = 1u16.saturating_add(chrome.header_gap);
    if chrome.show_divider {
        rows = rows.saturating_add(1).saturating_add(chrome.body_gap);
    }
    rows
}

/// Inner body slot height (composable content area below the divider).
pub fn dialog_shell_body_height(chrome: &DialogChrome) -> u16 {
    chrome.min_content_height
}

/// Total outer height of a [`super::DialogShell`] including border and padding.
pub fn dialog_shell_outer_height(chrome: &DialogChrome) -> u16 {
    dialog_shell_body_height(chrome)
        .saturating_add(dialog_shell_chrome_rows(chrome))
        .saturating_add(chrome.padding_vertical.saturating_mul(2))
        .saturating_add(2)
}

/// Estimated outer height for centering overlays — alias of [`dialog_shell_outer_height`].
pub fn dialog_shell_estimated_height(chrome: &DialogChrome) -> u16 {
    dialog_shell_outer_height(chrome)
}

/// Viewport height for list-style dialog bodies after prompt + footer hints.
///
/// Prefer [`dialog_select_body_plan`] when option rows are known.
pub fn dialog_choice_list_height(chrome: &DialogChrome, reserved_rows: u16) -> u16 {
    chrome.min_content_height.saturating_sub(reserved_rows).max(4)
}

/// Wrapped line count for dialog prompt copy at the body width.
pub fn dialog_text_rows(text: &str, width: u16) -> u16 {
    if text.is_empty() {
        0
    } else {
        wrapped_transcript_row_count(text, width.max(1)).max(1)
    }
}

/// Extra rows reserved around a flat [`crate::components::SelectList`] body (no list border).
pub fn select_list_chrome_rows(_theme: UiTheme) -> u16 {
    0
}

/// Max inner body rows that fit on screen inside a [`super::DialogShell`].
pub fn dialog_max_content_height(screen_height: u16, chrome: &DialogChrome, vertical_margin: u16) -> u16 {
    screen_height
        .saturating_sub(vertical_margin)
        .saturating_sub(dialog_shell_chrome_rows(chrome))
        .saturating_sub(chrome.padding_vertical.saturating_mul(2))
        .saturating_sub(2)
        .max(4)
}

/// Fixed rows in a select-style dialog body excluding the scrollable list viewport.
pub fn dialog_select_fixed_rows(intro: &str, list_width: u16, theme: UiTheme, trailing_rows: u16) -> u16 {
    let mut rows = select_list_chrome_rows(theme);
    let intro_rows = dialog_text_rows(intro, list_width);
    if intro_rows > 0 {
        rows = rows
            .saturating_add(intro_rows)
            .saturating_add(theme.dialog_section_gap());
    }
    if trailing_rows > 0 {
        rows = rows
            .saturating_add(trailing_rows)
            .saturating_add(theme.dialog_section_gap());
    }
    rows
}

/// Plan shell body height and list viewport from real option rows.
///
/// Returns `(min_content_height, list_viewport_height)`. `list_viewport_height` is
/// [`SELECT_LIST_AUTO_HEIGHT`] when the list should grow to fit all options; otherwise a
/// capped viewport height for in-list scrolling.
#[allow(clippy::too_many_arguments)]
pub fn dialog_select_body_plan(
    options: &[SelectOption],
    show_description: bool,
    list_width: u16,
    theme: UiTheme,
    intro: &str,
    trailing_rows: u16,
    max_body_height: Option<u16>,
    compact: bool,
) -> (u16, u16) {
    let list_rows = select_list_total_rows(options, show_description, list_width, theme, compact) as u16;
    let fixed_rows = dialog_select_fixed_rows(intro, list_width, theme, trailing_rows);
    let natural_body = fixed_rows.saturating_add(list_rows);
    let body_min = dialog_body_min_height(natural_body);

    let list_viewport = match max_body_height {
        Some(cap) if natural_body > cap => cap.saturating_sub(fixed_rows).max(4),
        _ => SELECT_LIST_AUTO_HEIGHT,
    };

    let body_height = match max_body_height {
        Some(cap) if natural_body > cap => cap,
        _ => body_min,
    };

    (body_height, list_viewport)
}

/// Row count for a todo checklist body (label + wrapped detail lines).
pub fn dialog_todo_list_content_rows(items: &[DialogTodoItem], list_width: u16, theme: UiTheme, row_gap: u16) -> u16 {
    if items.is_empty() {
        return 4;
    }
    let detail_width = list_width.saturating_sub(theme.detail_padding_left()).max(1);
    let mut rows = 0u16;
    for item in items {
        rows = rows.saturating_add(1);
        if !item.detail.is_empty() {
            rows = rows.saturating_add(wrapped_transcript_row_count(&item.detail, detail_width));
        }
    }
    let gaps = row_gap.saturating_mul((items.len() as u16).saturating_sub(1));
    rows.saturating_add(gaps).max(4)
}

/// Auto-height sentinel for dialog-embedded [`crate::components::SelectList`].
pub const DIALOG_SELECT_AUTO_HEIGHT: u16 = SELECT_LIST_AUTO_HEIGHT;

/// Suggested `min_content_height` from an estimated body row count.
pub fn dialog_body_min_height(content_rows: u16) -> u16 {
    content_rows.max(4)
}

/// Horizontal rule matching the dialog content width.
pub fn dialog_divider_line(width: u16) -> String {
    "─".repeat(width.max(1) as usize)
}

/// Truncate a title so the esc hint fits on one row.
pub fn dialog_header_title_fit(title: &str, content_width: u16, esc_hint: &str) -> String {
    let esc_len = esc_hint.chars().count();
    let max_title = content_width.saturating_sub(esc_len as u16 + 1) as usize;
    if max_title == 0 {
        return String::new();
    }
    let chars: Vec<char> = title.chars().collect();
    if chars.len() <= max_title {
        return title.to_string();
    }
    if max_title <= 1 {
        return "…".to_string();
    }
    chars.into_iter().take(max_title - 1).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider_matches_width() {
        assert_eq!(dialog_divider_line(10).chars().count(), 10);
    }

    #[test]
    fn title_truncates_for_esc_hint() {
        let title = dialog_header_title_fit("Very long dialog title here", 20, "[esc]");
        assert!(title.chars().count() <= 14);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn content_width_accounts_for_padding() {
        let chrome = DialogChrome {
            width: 40,
            padding_horizontal: 1,
            ..Default::default()
        };
        assert_eq!(chrome.content_width(), 36);
        assert_eq!(chrome.inner_body_width(), 36);
    }

    #[test]
    fn default_spacing_uses_theme_rhythm() {
        let theme = UiTheme::default();
        let chrome = DialogChrome::default();
        assert_eq!(chrome.padding_vertical, theme.dialog_shell_inset_vertical());
        assert_eq!(chrome.padding_horizontal, theme.dialog_shell_inset_horizontal());
        assert_eq!(chrome.header_gap, theme.dialog_header_gap());
        assert_eq!(chrome.body_gap, theme.dialog_section_gap());
        assert_eq!(chrome.row_gap, theme.dialog_row_gap());
    }

    #[test]
    fn chrome_rows_match_frame_layout() {
        let chrome = DialogChrome::default();
        assert_eq!(dialog_shell_chrome_rows(&chrome), 3);
    }

    #[test]
    fn slim_header_omits_divider_and_gaps() {
        let chrome = DialogChrome {
            slim_header: true,
            ..Default::default()
        };
        let themed = chrome.with_theme(UiTheme::default());
        assert_eq!(dialog_shell_chrome_rows(&themed), 1);
        assert!(!themed.show_divider);
        assert_eq!(themed.padding_vertical, 0);
        assert_eq!(themed.header_gap, 0);
    }

    #[test]
    fn outer_height_includes_border_and_padding() {
        let chrome = DialogChrome {
            min_content_height: 8,
            padding_vertical: 2,
            ..Default::default()
        };
        assert_eq!(dialog_shell_outer_height(&chrome), 8 + 3 + 4 + 2);
    }

    #[test]
    fn choice_list_height_reserves_prompt_rows() {
        let chrome = DialogChrome {
            min_content_height: 10,
            ..Default::default()
        };
        assert_eq!(dialog_choice_list_height(&chrome, 3), 7);
    }

    #[test]
    fn todo_list_rows_include_details_and_gaps() {
        let theme = UiTheme::default();
        let items = vec![
            DialogTodoItem::new("done", crate::types::DialogTodoStatus::Done),
            DialogTodoItem::new("pending", crate::types::DialogTodoStatus::Pending).with_detail("extra line"),
        ];
        assert_eq!(dialog_todo_list_content_rows(&items, 40, theme, theme.dialog_row_gap()), 4);
    }

    #[test]
    fn select_body_plan_fits_mode_options() {
        let theme = UiTheme::default();
        let options = crate::types::DialogAgentMode::all()
            .into_iter()
            .map(|mode| SelectOption::new(mode.label(), mode.description()))
            .collect::<Vec<_>>();
        let (body_h, list_h) = dialog_select_body_plan(
            &options,
            true,
            48,
            theme,
            "Choose how much autonomy the agent has for this session.",
            0,
            None,
            false,
        );
        assert_eq!(list_h, SELECT_LIST_AUTO_HEIGHT);
        assert!(body_h >= 8);
    }

    #[test]
    fn select_body_plan_caps_list_when_screen_is_short() {
        let theme = UiTheme::default();
        let options = crate::types::DialogAgentMode::all()
            .into_iter()
            .map(|mode| SelectOption::new(mode.label(), mode.description()))
            .collect::<Vec<_>>();
        let (body_h, list_h) = dialog_select_body_plan(
            &options,
            true,
            48,
            theme,
            "Choose how much autonomy the agent has for this session.",
            0,
            Some(10),
            false,
        );
        assert!(list_h > 0);
        assert_ne!(list_h, SELECT_LIST_AUTO_HEIGHT);
        assert_eq!(body_h, 10);
    }
}
