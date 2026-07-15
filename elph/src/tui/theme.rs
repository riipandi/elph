//! Shared terminal colors for the Elph shell.
//!
//! Color reference: https://www.ditig.com/256-colors-cheat-sheet

use iocraft::prelude::Color;

pub const BORDER_MUTED: Color = Color::Rgb { r: 88, g: 88, b: 88 };
pub const SCROLLBAR_TRACK: Color = Color::Rgb { r: 48, g: 48, b: 48 };
pub const BUBBLE_BG: Color = Color::Rgb { r: 48, g: 48, b: 48 };
pub const TOOL_BG: Color = Color::Rgb { r: 0, g: 95, b: 175 };
pub const EDITOR_BORDER: Color = Color::Rgb { r: 108, g: 108, b: 108 };

pub fn rgb_color((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb { r, g, b }
}
