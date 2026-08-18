//! Map [`UiTheme`] onto rendown's neutral [`MarkdownTheme`].

use rendown::{FontWeight, MarkdownTheme};

use super::blocks::CODE_BLOCK_INSET_H;
use super::convert::from_iocraft_color;
use crate::components::theme::UiTheme;

/// Semantic markdown palette derived from [`UiTheme`].
pub fn theme_from_ui(theme: UiTheme) -> MarkdownTheme {
    MarkdownTheme {
        body: from_iocraft_color(theme.text_primary),
        heading: from_iocraft_color(theme.warning),
        heading_weight: FontWeight::Bold,
        strong: from_iocraft_color(theme.text_primary),
        emphasis: from_iocraft_color(theme.text_secondary),
        inline_code: from_iocraft_color(theme.success),
        link: from_iocraft_color(theme.accent),
        code_bg: from_iocraft_color(theme.code_block_bg),
        code_inset: CODE_BLOCK_INSET_H,
        blockquote: from_iocraft_color(theme.text_muted),
        horizontal_rule: from_iocraft_color(theme.text_hint),
        list_marker: from_iocraft_color(theme.accent_soft),
        table_border: from_iocraft_color(theme.border),
        table_header: from_iocraft_color(theme.warning),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_bg_uses_dedicated_block_surface() {
        let ui = UiTheme::default();
        let md = theme_from_ui(ui);
        assert_eq!(md.code_bg, from_iocraft_color(ui.code_block_bg));
        assert_ne!(md.code_bg, from_iocraft_color(ui.selection_bg));
    }
}
