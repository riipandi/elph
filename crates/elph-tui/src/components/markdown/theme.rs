//! Pi-aligned markdown colors for transcript rendering.

use iocraft::prelude::{Color, Weight};

/// Semantic markdown palette (Pi `dark` theme).
#[derive(Clone, Copy, Debug)]
pub struct MarkdownTheme {
    pub body: Color,
    pub heading: Color,
    pub heading_weight: Weight,
    pub strong: Color,
    pub emphasis: Color,
    pub inline_code: Color,
    pub link: Color,
    pub code_bg: Color,
    pub blockquote: Color,
    pub list_marker: Color,
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self {
            body: Color::Rgb { r: 212, g: 212, b: 212 },
            heading: Color::Rgb { r: 240, g: 198, b: 116 },
            heading_weight: Weight::Bold,
            strong: Color::Rgb { r: 255, g: 255, b: 255 },
            emphasis: Color::Rgb { r: 200, g: 200, b: 200 },
            inline_code: Color::Rgb { r: 181, g: 189, b: 104 },
            link: Color::Rgb { r: 129, g: 161, b: 193 },
            code_bg: Color::Rgb { r: 40, g: 40, b: 48 },
            blockquote: Color::Rgb { r: 160, g: 160, b: 160 },
            list_marker: Color::Rgb { r: 149, g: 117, b: 205 },
        }
    }
}
