//! Color helpers for terminal UI components.

use iocraft::prelude::Color;

/// Parse a `#RRGGBB` hex color into an iocraft RGB color.
pub fn from_hex(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(rgb(r, g, b))
}

/// Build an iocraft RGB color.
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb { r, g, b }
}
