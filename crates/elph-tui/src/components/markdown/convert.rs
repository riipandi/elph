//! Neutral rendown colors ↔ iocraft paint types.

use iocraft::prelude::{Color, Weight};
use rendown::{FontWeight, RgbColor};

pub fn to_iocraft_color(color: RgbColor) -> Color {
    Color::Rgb {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

pub fn from_iocraft_color(color: Color) -> RgbColor {
    match color {
        Color::Rgb { r, g, b } => RgbColor::new(r, g, b),
        _ => RgbColor::new(0xd4, 0xd5, 0xd9),
    }
}

pub fn to_iocraft_weight(weight: FontWeight) -> Weight {
    match weight {
        FontWeight::Bold => Weight::Bold,
        FontWeight::Normal => Weight::Normal,
    }
}
