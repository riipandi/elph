//! Layout helpers — anchor the palette above the editor without covering it.

use elph_tui::components::UiTheme;
use elph_tui::layout_textarea;
use elph_tui::{PREFIX_COLUMN_WIDTH, PromptPrefixConfig};

/// Gap between palette bottom edge and editor top edge (terminal rows).
pub const PALETTE_EDITOR_GAP: u16 = 0;

/// Editor max height — kept in sync with [`super::super::editor`].
pub fn editor_max_height(screen_height: u16) -> u16 {
    (screen_height / 4).clamp(4, 12)
}

fn prompt_textarea_width(screen_width: u16) -> u16 {
    let theme = UiTheme::default();
    let prefix_cols = if PromptPrefixConfig::default().enabled {
        PREFIX_COLUMN_WIDTH
    } else {
        0
    };
    theme
        .shell_editor_inner_width(screen_width)
        .saturating_sub(prefix_cols)
        .max(1)
}

/// Visible editor block height in rows (border + textarea viewport).
pub fn editor_chrome_height(draft: &str, screen_width: u16, screen_height: u16) -> u16 {
    let textarea_width = prompt_textarea_width(screen_width);
    let max_height = Some(editor_max_height(screen_height));
    let cursor = draft.len();
    let layout = layout_textarea(draft, cursor, textarea_width, 1, max_height);
    layout.viewport_height.saturating_add(2)
}

/// `bottom` offset for an absolutely positioned palette sitting above the editor.
pub fn palette_anchor_bottom(draft: &str, screen_width: u16, screen_height: u16) -> u16 {
    editor_chrome_height(draft, screen_width, screen_height).saturating_add(PALETTE_EDITOR_GAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_sits_above_single_line_editor() {
        let anchor = palette_anchor_bottom("", 80, 40);
        assert!(anchor >= 2);
    }

    #[test]
    fn anchor_grows_with_multiline_draft() {
        let single = editor_chrome_height("one line", 80, 40);
        let multi = editor_chrome_height("line one\nline two\nline three", 80, 40);
        assert!(multi >= single);
    }
}
