//! Markdown colors for terminal rendering (Ghostty dark defaults).

use crate::model::{FontWeight, RgbColor};

/// Semantic markdown palette (hard-coded Ghostty-like dark theme).
#[derive(Clone, Copy, Debug)]
pub struct MarkdownTheme {
    pub body: RgbColor,
    pub heading: RgbColor,
    pub heading_weight: FontWeight,
    pub strong: RgbColor,
    pub emphasis: RgbColor,
    pub inline_code: RgbColor,
    pub link: RgbColor,
    pub code_bg: RgbColor,
    pub code_inset: u16,
    pub blockquote: RgbColor,
    pub horizontal_rule: RgbColor,
    pub list_marker: RgbColor,
    pub table_border: RgbColor,
    pub table_header: RgbColor,
}

impl MarkdownTheme {
    /// Ghostty-style dark palette (matches elph-tui [`UiTheme::dark`] mapping).
    pub const fn dark() -> Self {
        Self {
            // text_primary #d4d5d9
            body: RgbColor::new(0xd4, 0xd5, 0xd9),
            // warning #ffb347
            heading: RgbColor::new(0xff, 0xb3, 0x47),
            heading_weight: FontWeight::Bold,
            strong: RgbColor::new(0xd4, 0xd5, 0xd9),
            // text_secondary
            emphasis: RgbColor::new(0xb0, 0xb3, 0xb9),
            // success #8ed16a
            inline_code: RgbColor::new(0x8e, 0xd1, 0x6a),
            // accent #6699ff
            link: RgbColor::new(0x66, 0x99, 0xff),
            // code_block_bg #191a1c
            code_bg: RgbColor::new(0x19, 0x1a, 0x1c),
            code_inset: crate::blocks::CODE_BLOCK_INSET_H,
            // text_muted
            blockquote: RgbColor::new(0x7a, 0x7e, 0x85),
            // text_hint
            horizontal_rule: RgbColor::new(0x5c, 0x60, 0x66),
            // accent_soft #4dd0e1
            list_marker: RgbColor::new(0x4d, 0xd0, 0xe1),
            // border
            table_border: RgbColor::new(0x3a, 0x3d, 0x42),
            table_header: RgbColor::new(0xff, 0xb3, 0x47),
        }
    }
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_dark() {
        let md = MarkdownTheme::default();
        assert_eq!(md.body, MarkdownTheme::dark().body);
        assert_ne!(md.code_bg, md.body);
    }
}
