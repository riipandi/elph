//! Terminal color adaptation for markdown spans (`supports-color` + `anstyle`).

use std::sync::OnceLock;

use anstyle::{Ansi256Color, AnsiColor, Color as AnstyleColor, Effects, RgbColor};
use anstyle_syntect::to_anstyle;
use iocraft::prelude::{Color, Weight};
use syntect::highlighting::Style as SyntectStyle;

use super::model::StyledSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ColorLevel {
    None,
    Basic,
    Ansi256,
    #[default]
    TrueColor,
}

static COLOR_LEVEL: OnceLock<ColorLevel> = OnceLock::new();

pub fn detect_color_level() -> ColorLevel {
    *COLOR_LEVEL.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return ColorLevel::None;
        }
        match supports_color::on(supports_color::Stream::Stdout) {
            None => ColorLevel::TrueColor,
            Some(level) => {
                if level.has_16m {
                    ColorLevel::TrueColor
                } else if level.has_256 {
                    ColorLevel::Ansi256
                } else if level.has_basic {
                    ColorLevel::Basic
                } else {
                    ColorLevel::None
                }
            }
        }
    })
}

fn adapt_anstyle_color(color: AnstyleColor) -> Option<AnstyleColor> {
    match detect_color_level() {
        ColorLevel::None => None,
        ColorLevel::TrueColor => Some(color),
        ColorLevel::Ansi256 => Some(match color {
            AnstyleColor::Rgb(rgb) => AnstyleColor::Ansi256(rgb_to_ansi256(rgb)),
            other => other,
        }),
        ColorLevel::Basic => Some(match color {
            AnstyleColor::Rgb(rgb) => AnstyleColor::Ansi(rgb_to_ansi16(rgb)),
            AnstyleColor::Ansi256(index) => AnstyleColor::Ansi(ansi256_to_ansi16(index)),
            AnstyleColor::Ansi(ansi) => AnstyleColor::Ansi(ansi),
        }),
    }
}

fn rgb_to_ansi256(rgb: RgbColor) -> Ansi256Color {
    let (r, g, b) = (rgb.0, rgb.1, rgb.2);
    if r == g && g == b {
        if r < 8 {
            return Ansi256Color(16);
        }
        if r > 248 {
            return Ansi256Color(231);
        }
        return Ansi256Color(232 + (r - 8) / 10);
    }
    Ansi256Color(16 + 36 * (r / 51) + 6 * (g / 51) + (b / 51))
}

fn rgb_to_ansi16(rgb: RgbColor) -> AnsiColor {
    let (r, g, b) = (rgb.0, rgb.1, rgb.2);
    if r > 127 && g < 64 && b < 64 {
        AnsiColor::Red
    } else if r < 64 && g > 127 && b < 64 {
        AnsiColor::Green
    } else if r < 64 && g < 64 && b > 127 {
        AnsiColor::Blue
    } else if r > 200 && g > 200 && b > 200 {
        AnsiColor::White
    } else if r < 64 && g < 64 && b < 64 {
        AnsiColor::Black
    } else if r > 127 || g > 127 || b > 127 {
        AnsiColor::BrightWhite
    } else {
        AnsiColor::White
    }
}

fn ansi256_to_ansi16(index: Ansi256Color) -> AnsiColor {
    let idx = index.index();
    if idx < 16 {
        match idx {
            0 => AnsiColor::Black,
            1 => AnsiColor::Red,
            2 => AnsiColor::Green,
            3 => AnsiColor::Yellow,
            4 => AnsiColor::Blue,
            5 => AnsiColor::Magenta,
            6 => AnsiColor::Cyan,
            7 => AnsiColor::White,
            8 => AnsiColor::BrightBlack,
            9 => AnsiColor::BrightRed,
            10 => AnsiColor::BrightGreen,
            11 => AnsiColor::BrightYellow,
            12 => AnsiColor::BrightBlue,
            13 => AnsiColor::BrightMagenta,
            14 => AnsiColor::BrightCyan,
            _ => AnsiColor::BrightWhite,
        }
    } else {
        AnsiColor::White
    }
}

fn anstyle_color_to_iocraft(color: AnstyleColor) -> Color {
    match color {
        AnstyleColor::Rgb(rgb) => Color::Rgb {
            r: rgb.0,
            g: rgb.1,
            b: rgb.2,
        },
        AnstyleColor::Ansi(ansi) => match ansi {
            AnsiColor::Black | AnsiColor::BrightBlack => Color::DarkGrey,
            AnsiColor::Red | AnsiColor::BrightRed => Color::DarkRed,
            AnsiColor::Green | AnsiColor::BrightGreen => Color::DarkGreen,
            AnsiColor::Yellow | AnsiColor::BrightYellow => Color::Yellow,
            AnsiColor::Blue | AnsiColor::BrightBlue => Color::Blue,
            AnsiColor::Magenta | AnsiColor::BrightMagenta => Color::Magenta,
            AnsiColor::Cyan | AnsiColor::BrightCyan => Color::Cyan,
            AnsiColor::White | AnsiColor::BrightWhite => Color::Grey,
        },
        AnstyleColor::Ansi256(index) => {
            if let Some(adapted) = adapt_anstyle_color(AnstyleColor::Ansi256(index)) {
                return anstyle_color_to_iocraft(adapted);
            }
            Color::Grey
        }
    }
}

pub fn syntect_to_styled_span(style: SyntectStyle, text: impl Into<String>, fallback: Color) -> StyledSpan {
    let anstyle = to_anstyle(style);
    let color = anstyle
        .get_fg_color()
        .and_then(adapt_anstyle_color)
        .map(anstyle_color_to_iocraft)
        .unwrap_or(fallback);
    let effects = anstyle.get_effects();
    StyledSpan {
        text: text.into(),
        color,
        weight: if effects.contains(Effects::BOLD) {
            Weight::Bold
        } else {
            Weight::Normal
        },
        italic: effects.contains(Effects::ITALIC),
    }
}
