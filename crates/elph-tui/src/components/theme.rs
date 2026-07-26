//! Shared visual tokens for elph-tui components.
//!
//! Default palette follows a Ghostty-style dark theme (cool charcoal base, soft
//! ANSI accents). Spacing is a 1:2:3 cell modular scale so borders and gutters
//! stay aligned across widgets.

use crate::color::rgb;
use crate::input_prefix::LIST_SELECTION_MARKER;
use iocraft::hooks::UseContext;
use iocraft::prelude::*;

// ── Ghostty dark base (canonical hex → RGB) ───────────────────────────────
// background #181a1d · foreground #d4d5d9 · selection #336ff1
// ANSI 0–15 from the host Ghostty config (see elph TUI theme docs).

/// Cohesive terminal palette and spacing for interactive components.
///
/// Spacing follows a 1:2:3 cell modular scale (`padding_sm` / `padding_md` / `padding_lg`)
/// so borders, gutters, and content columns stay visually aligned across widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiTheme {
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_hint: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub border: Color,
    pub border_focus: Color,
    pub border_subtle: Color,
    /// Round shell chrome (prompt editor, inline dialogs) when the zone has focus.
    pub shell_border: Color,
    /// Round shell chrome when another zone has focus.
    pub shell_border_dimmed: Color,
    pub surface: Color,
    /// Fenced code blocks in markdown (near-black charcoal; low chroma).
    pub code_block_bg: Color,
    pub selection_bg: Color,
    /// Soft amber highlight for selected inline dialog choices (ask-user, etc.).
    pub dialog_selection_bg: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub gap_sm: u16,
    pub gap_md: u16,
    pub gap_lg: u16,
    pub padding_sm: u16,
    pub padding_md: u16,
    pub padding_lg: u16,
}

impl UiTheme {
    /// Ghostty-style dark palette (default Elph appearance).
    pub const fn dark() -> Self {
        Self {
            // foreground #d4d5d9
            text_primary: rgb(0xd4, 0xd5, 0xd9),
            text_secondary: rgb(0xb0, 0xb3, 0xb9),
            // palette 8 #7a7e85
            text_muted: rgb(0x7a, 0x7e, 0x85),
            text_hint: rgb(0x5c, 0x60, 0x66),
            // palette 4 #6699ff
            accent: rgb(0x66, 0x99, 0xff),
            // palette 6 #4dd0e1
            accent_soft: rgb(0x4d, 0xd0, 0xe1),
            border: rgb(0x3a, 0x3d, 0x42),
            border_focus: rgb(0x66, 0x99, 0xff),
            // palette 0 #191a1c
            border_subtle: rgb(0x19, 0x1a, 0x1c),
            shell_border: rgb(0x7a, 0x7e, 0x85),
            shell_border_dimmed: rgb(0x3a, 0x3d, 0x42),
            surface: Color::Reset,
            code_block_bg: rgb(0x19, 0x1a, 0x1c),
            selection_bg: rgb(0x22, 0x28, 0x36),
            dialog_selection_bg: rgb(0x3a, 0x32, 0x22),
            // palette 2 #8ed16a
            success: rgb(0x8e, 0xd1, 0x6a),
            // palette 3 #ffb347
            warning: rgb(0xff, 0xb3, 0x47),
            // palette 1 #ff6b66
            error: rgb(0xff, 0x6b, 0x66),
            gap_sm: 0,
            gap_md: 1,
            gap_lg: 2,
            padding_sm: 1,
            padding_md: 2,
            padding_lg: 3,
        }
    }

    /// Light palette — cool paper background, charcoal text, same accent family.
    pub const fn light() -> Self {
        Self {
            text_primary: rgb(0x1a, 0x1b, 0x1e),
            text_secondary: rgb(0x3a, 0x3d, 0x44),
            text_muted: rgb(0x5c, 0x60, 0x66),
            text_hint: rgb(0x8a, 0x8e, 0x95),
            accent: rgb(0x33, 0x6f, 0xf1),
            accent_soft: rgb(0x0e, 0xa5, 0xb5),
            border: rgb(0xc8, 0xcb, 0xd1),
            border_focus: rgb(0x33, 0x6f, 0xf1),
            border_subtle: rgb(0xe4, 0xe6, 0xea),
            shell_border: rgb(0x7a, 0x7e, 0x85),
            shell_border_dimmed: rgb(0xc8, 0xcb, 0xd1),
            surface: Color::Reset,
            code_block_bg: rgb(0xe8, 0xea, 0xed),
            selection_bg: rgb(0xde, 0xe7, 0xfc),
            dialog_selection_bg: rgb(0xff, 0xf0, 0xd6),
            success: rgb(0x3d, 0x9a, 0x3a),
            warning: rgb(0xc4, 0x7d, 0x12),
            error: rgb(0xd0, 0x3f, 0x3f),
            gap_sm: 0,
            gap_md: 1,
            gap_lg: 2,
            padding_sm: 1,
            padding_md: 2,
            padding_lg: 3,
        }
    }
}

impl Default for UiTheme {
    /// Process-active theme if set, otherwise [`UiTheme::dark`].
    fn default() -> Self {
        crate::theme_config::active_ui_theme()
    }
}

/// Marker column width (`❯` + gap) for list rows.
pub const LIST_MARKER_COL: u16 = 2;

/// Horizontal chrome consumed by a single-line border plus inner inset.
pub const BORDER_CHROME_COLS: u16 = 2;

impl UiTheme {
    pub fn focus_border(self, has_focus: bool) -> BorderStyle {
        if has_focus {
            BorderStyle::Round
        } else {
            BorderStyle::None
        }
    }

    /// Bordered container chrome (round when focused, single when not).
    pub fn container_border(self, has_focus: bool) -> BorderStyle {
        if has_focus {
            BorderStyle::Round
        } else {
            BorderStyle::Single
        }
    }

    pub fn focus_border_color(self, has_focus: bool) -> Color {
        if has_focus {
            self.border_focus
        } else {
            self.border_subtle
        }
    }

    pub fn container_border_color(self, has_focus: bool) -> Color {
        if has_focus { self.border_focus } else { self.border }
    }

    /// Border color for full-width shell zones (prompt editor, inline dialogs).
    pub fn shell_zone_border_color(self, has_focus: bool) -> Color {
        if has_focus {
            self.shell_border
        } else {
            self.shell_border_dimmed
        }
    }

    pub fn list_marker_color(self, selected: bool) -> Color {
        if selected { self.accent_soft } else { self.text_hint }
    }

    /// Inner horizontal inset for bordered panels and list viewports.
    pub fn container_inset(self) -> u16 {
        self.padding_sm
    }

    /// Inner horizontal inset for single-line inputs.
    pub fn input_inset(self) -> u16 {
        self.padding_sm
    }

    /// Gap between a line-number gutter and code body.
    pub fn gutter_gap(self) -> u16 {
        self.gap_md
    }

    /// Minimum content rows for tab panels and compact containers.
    pub fn panel_min_height(self) -> u16 {
        3
    }

    /// Inner width for list row content (horizontal row inset only).
    pub fn list_viewport_inner_width(self, outer_width: u16) -> u16 {
        outer_width
            .saturating_sub(self.container_inset().saturating_mul(2))
            .max(1)
    }

    /// Left inset for wrapped description lines under a list row name.
    pub fn list_desc_padding_left(self) -> u16 {
        LIST_MARKER_COL.saturating_add(self.gap_md)
    }

    /// Detail line indent for checklist-style rows (marker + gap).
    pub fn detail_padding_left(self) -> u16 {
        self.list_desc_padding_left().saturating_add(self.gap_md)
    }

    /// Transparent/default fill for inline panels and lists.
    pub fn list_surface(self) -> Color {
        self.surface
    }

    /// Opaque fill for modal dialog cards (avoids bleed-through from content behind).
    pub fn dialog_surface(self) -> Color {
        self.selection_bg
    }

    /// Horizontal inset shared by shell chrome zones (header, transcript, status, prompt).
    pub fn shell_zone_padding(self) -> u16 {
        self.padding_sm
    }

    /// Editor inner width inside a full-width round border with zone padding.
    pub fn shell_editor_inner_width(self, screen_width: u16) -> u16 {
        screen_width
            .saturating_sub(BORDER_CHROME_COLS)
            .saturating_sub(self.shell_zone_padding().saturating_mul(2))
            .max(1)
    }

    /// Inner inset between dialog round border and content (uniform fallback).
    pub fn dialog_shell_inset(self) -> u16 {
        self.dialog_shell_inset_horizontal()
    }

    /// Horizontal inset inside the dialog round border.
    pub fn dialog_shell_inset_horizontal(self) -> u16 {
        self.padding_md
    }

    /// Vertical inset inside the dialog round border (tighter than horizontal).
    pub fn dialog_shell_inset_vertical(self) -> u16 {
        self.padding_sm
    }

    /// Opaque fill for dialog-embedded lists and panels.
    pub fn dialog_content_surface(self) -> Color {
        self.dialog_surface()
    }

    /// Gap below the header row, before the divider.
    pub fn dialog_header_gap(self) -> u16 {
        self.gap_sm
    }

    /// Gap between major dialog body blocks (prompt, list, actions).
    pub fn dialog_section_gap(self) -> u16 {
        self.gap_md
    }

    /// Gap between stacked rows inside list-style dialog bodies.
    pub fn dialog_row_gap(self) -> u16 {
        self.gap_sm
    }

    /// Alias of [`Self::dialog_section_gap`] — use flex `gap` only, never stack with padding.
    pub fn dialog_action_gap(self) -> u16 {
        self.dialog_section_gap()
    }

    /// Editable text — primary when focused, dimmed grey when inactive.
    pub fn input_text_color(self, has_focus: bool) -> Color {
        if has_focus { self.text_primary } else { self.text_muted }
    }

    /// High-contrast caret block for focused editors (Ghostty cursor-color #ffffff).
    pub fn input_cursor_color(self) -> Color {
        Color::White
    }

    /// Ask-user dialog fields — warm warning accent when focused.
    pub fn dialog_input_text_color(self, has_focus: bool) -> Color {
        if has_focus {
            self.text_primary
        } else {
            self.text_secondary
        }
    }

    pub fn dialog_input_cursor_color(self) -> Color {
        self.warning
    }

    pub fn dialog_input_underline_color(self, has_focus: bool) -> Color {
        if has_focus { self.warning } else { self.border_subtle }
    }

    pub fn input_border_color(self, has_focus: bool) -> Color {
        if has_focus {
            self.border_focus
        } else {
            self.border_subtle
        }
    }

    pub fn scrollbar_style(self) -> super::scroll_bar::ScrollbarStyle {
        super::scroll_bar::ScrollbarStyle {
            thumb_color: Some(self.border_focus),
            track_color: Some(self.border_subtle),
        }
    }
}

/// Marker column for a selected list row.
pub fn list_marker(selected: bool) -> &'static str {
    if selected { LIST_SELECTION_MARKER } else { " " }
}

/// Name style for one list row.
pub fn list_row_name_style(theme: UiTheme, selected: bool) -> (Color, Weight) {
    if selected {
        (theme.text_primary, Weight::Bold)
    } else {
        (theme.text_secondary, Weight::Normal)
    }
}

/// Description style for one list row.
pub fn list_row_desc_style(theme: UiTheme, selected: bool) -> Color {
    if selected {
        theme.text_secondary
    } else {
        theme.text_muted
    }
}

/// Background for a selected inline dialog choice row.
pub fn dialog_row_surface(theme: UiTheme, selected: bool) -> Color {
    if selected {
        theme.dialog_selection_bg
    } else {
        Color::Reset
    }
}

/// Marker color for inline dialog lists (`❯` beside the active choice).
pub fn dialog_marker_color(theme: UiTheme, selected: bool) -> Color {
    if selected { theme.warning } else { theme.text_hint }
}

/// Inline dialog option name — bold; soft yellow when selected, secondary otherwise.
pub fn dialog_option_name_style(theme: UiTheme, selected: bool) -> (Color, Weight) {
    let color = if selected { theme.warning } else { theme.text_secondary };
    (color, Weight::Bold)
}

/// Inline dialog option detail — dimmest when idle, slightly lifted when the row is selected.
pub fn dialog_option_desc_style(theme: UiTheme, selected: bool) -> Color {
    if selected { theme.text_muted } else { theme.text_hint }
}

/// Tab chrome for horizontal selectors.
pub fn tab_styles(theme: UiTheme, active: bool) -> (BorderStyle, Color, Weight) {
    if active {
        (BorderStyle::Round, theme.accent_soft, Weight::Bold)
    } else {
        (BorderStyle::None, theme.text_muted, Weight::Normal)
    }
}

/// Resolve the active theme: prop → process theme (settings / Ctrl+Shift+T) → provider context → dark.
pub fn resolve_ui_theme(hooks: &Hooks, prop: Option<UiTheme>) -> UiTheme {
    prop.or_else(crate::theme_config::try_active_ui_theme)
        .or_else(|| hooks.try_use_context::<UiTheme>().map(|theme| *theme))
        .unwrap_or_else(UiTheme::dark)
}

/// Props for [`UiThemeProvider`] — app-level theme (React `ThemeProvider` analogue).
#[derive(Default, Props)]
pub struct UiThemeProviderProps<'a> {
    pub theme: UiTheme,
    pub children: Vec<AnyElement<'a>>,
}

/// Supplies a default [`UiTheme`] to all descendant components via iocraft context.
///
/// Per-component `theme: Some(...)` props still override this value.
#[component]
pub fn UiThemeProvider<'a>(props: &mut UiThemeProviderProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = props.theme;
    let children = std::mem::take(&mut props.children);
    element! {
        ContextProvider(value: Context::owned(theme)) {
            #(children)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_differ_for_selection() {
        assert_eq!(list_marker(true), LIST_SELECTION_MARKER);
        assert_eq!(list_marker(false), " ");
    }

    #[test]
    fn selected_name_is_bold_primary() {
        let theme = UiTheme::default();
        assert_eq!(list_row_name_style(theme, true), (theme.text_primary, Weight::Bold));
    }

    #[test]
    fn dialog_option_name_is_always_bold() {
        let theme = UiTheme::default();
        assert_eq!(dialog_option_name_style(theme, false), (theme.text_secondary, Weight::Bold));
        assert_eq!(dialog_option_name_style(theme, true), (theme.warning, Weight::Bold));
    }

    #[test]
    fn dialog_option_desc_lifts_slightly_when_selected() {
        let theme = UiTheme::default();
        assert_eq!(dialog_option_desc_style(theme, false), theme.text_hint);
        assert_eq!(dialog_option_desc_style(theme, true), theme.text_muted);
    }

    #[test]
    fn container_border_round_when_focused() {
        let theme = UiTheme::default();
        assert_eq!(theme.container_border(true), BorderStyle::Round);
        assert_eq!(theme.container_border(false), BorderStyle::Single);
    }

    #[test]
    fn list_desc_indent_aligns_with_name() {
        let theme = UiTheme::default();
        assert_eq!(theme.list_desc_padding_left(), LIST_MARKER_COL + theme.gap_md);
    }

    #[test]
    fn spacing_scale_is_monotonic() {
        let theme = UiTheme::default();
        assert!(theme.gap_sm <= theme.gap_md);
        assert!(theme.gap_md <= theme.gap_lg);
        assert!(theme.padding_sm <= theme.padding_md);
        assert!(theme.padding_md <= theme.padding_lg);
    }

    #[test]
    fn text_roles_decrease_in_emphasis() {
        let theme = UiTheme::default();
        let primary = theme.text_primary;
        let secondary = theme.text_secondary;
        let muted = theme.text_muted;
        let hint = theme.text_hint;
        let lum = |c: Color| match c {
            Color::Rgb { r, g, b } => (r as u32 + g as u32 + b as u32) / 3,
            _ => 128,
        };
        assert!(lum(primary) >= lum(secondary));
        assert!(lum(secondary) >= lum(muted));
        assert!(lum(muted) >= lum(hint));
    }

    #[test]
    fn code_block_bg_differs_from_selection_and_user_cards() {
        let theme = UiTheme::default();
        assert_ne!(theme.code_block_bg, theme.selection_bg);
        let lum = |c: Color| match c {
            Color::Rgb { r, g, b } => (r as u32 + g as u32 + b as u32) / 3,
            _ => 128,
        };
        assert!(lum(theme.code_block_bg) < lum(theme.selection_bg));
        // Ghostty black (palette 0) — near-neutral charcoal, not a tinted wash.
        match theme.code_block_bg {
            Color::Rgb { r, g, b } => {
                assert!((r as i16 - g as i16).unsigned_abs() <= 2);
                assert!((g as i16 - b as i16).unsigned_abs() <= 2);
                assert!(lum(theme.code_block_bg) < 40);
            }
            _ => panic!("expected rgb code block background"),
        }
    }

    #[test]
    fn ghostty_canonical_tokens() {
        let theme = UiTheme::dark();
        assert_eq!(theme.text_primary, rgb(0xd4, 0xd5, 0xd9));
        assert_eq!(theme.accent, rgb(0x66, 0x99, 0xff));
        assert_eq!(theme.accent_soft, rgb(0x4d, 0xd0, 0xe1));
        assert_eq!(theme.success, rgb(0x8e, 0xd1, 0x6a));
        assert_eq!(theme.warning, rgb(0xff, 0xb3, 0x47));
        assert_eq!(theme.error, rgb(0xff, 0x6b, 0x66));
        assert_eq!(theme.input_cursor_color(), Color::White);
    }

    #[test]
    fn light_palette_is_dark_on_light() {
        let light = UiTheme::light();
        let dark = UiTheme::dark();
        let lum = |c: Color| match c {
            Color::Rgb { r, g, b } => (r as u32 + g as u32 + b as u32) / 3,
            _ => 128,
        };
        assert!(lum(light.text_primary) < lum(dark.text_primary));
        assert!(lum(light.code_block_bg) > lum(dark.code_block_bg));
    }

    #[test]
    fn inactive_input_text_is_dimmed() {
        let theme = UiTheme::default();
        assert_eq!(theme.input_text_color(true), theme.text_primary);
        assert_eq!(theme.input_text_color(false), theme.text_muted);
    }

    #[test]
    fn shell_zone_border_tracks_focus() {
        let theme = UiTheme::default();
        assert_eq!(theme.shell_zone_border_color(true), theme.shell_border);
        assert_eq!(theme.shell_zone_border_color(false), theme.shell_border_dimmed);
    }

    #[test]
    fn list_viewport_width_accounts_for_row_inset() {
        let theme = UiTheme::default();
        assert_eq!(theme.list_viewport_inner_width(20), 20 - theme.container_inset() * 2);
    }
}
