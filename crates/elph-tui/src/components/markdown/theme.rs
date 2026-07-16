//! Pi-aligned markdown colors for transcript rendering.

use crate::components::theme::UiTheme;
use iocraft::prelude::{Color, Weight};

/// Semantic markdown palette (Pi `dark` theme).
#[derive(Clone, Copy, Debug)]
pub struct MarkdownTheme {
    pub ui: UiTheme,
    pub body: Color,
    pub heading: Color,
    pub heading_weight: Weight,
    pub strong: Color,
    pub emphasis: Color,
    pub inline_code: Color,
    pub link: Color,
    pub code_bg: Color,
    pub code_inset: u16,
    pub blockquote: Color,
    pub list_marker: Color,
    pub table_border: Color,
    pub table_header: Color,
}

impl MarkdownTheme {
    pub fn from_ui_theme(theme: UiTheme) -> Self {
        Self {
            ui: theme,
            body: theme.text_primary,
            heading: theme.warning,
            heading_weight: Weight::Bold,
            strong: theme.text_primary,
            emphasis: theme.text_secondary,
            inline_code: theme.success,
            link: theme.accent,
            code_bg: theme.code_block_bg,
            code_inset: super::blocks::CODE_BLOCK_INSET_H,
            blockquote: theme.text_muted,
            list_marker: theme.accent_soft,
            table_border: theme.border,
            table_header: theme.warning,
        }
    }
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self::from_ui_theme(UiTheme::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_bg_uses_dedicated_block_surface() {
        let ui = UiTheme::default();
        let md = MarkdownTheme::from_ui_theme(ui);
        assert_eq!(md.code_bg, ui.code_block_bg);
        assert_ne!(md.code_bg, ui.selection_bg);
    }
}
