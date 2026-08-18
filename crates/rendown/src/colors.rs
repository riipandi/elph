//! Terminal color adaptation for markdown spans (`supports-color` + `anstyle`).

use std::sync::OnceLock;

use anstyle::{Ansi256Color, AnsiColor, Color as AnstyleColor, Effects, RgbColor as AnstyleRgb};
use anstyle_syntect::to_anstyle;
use syntect::highlighting::Style as SyntectStyle;

use crate::model::{FontWeight, RgbColor, StyledSpan};

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

fn adapt_anstyle_color(color: AnstyleColor, level: ColorLevel) -> Option<AnstyleColor> {
    match level {
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

fn rgb_to_ansi256(rgb: AnstyleRgb) -> Ansi256Color {
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

fn rgb_to_ansi16(rgb: AnstyleRgb) -> AnsiColor {
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

fn anstyle_color_to_rgb(color: AnstyleColor, fallback: RgbColor) -> RgbColor {
    match color {
        AnstyleColor::Rgb(rgb) => RgbColor::new(rgb.0, rgb.1, rgb.2),
        AnstyleColor::Ansi(ansi) => match ansi {
            AnsiColor::Black | AnsiColor::BrightBlack => RgbColor::new(0x7a, 0x7e, 0x85),
            AnsiColor::Red | AnsiColor::BrightRed => RgbColor::new(0xff, 0x6b, 0x66),
            AnsiColor::Green | AnsiColor::BrightGreen => RgbColor::new(0x8e, 0xd1, 0x6a),
            AnsiColor::Yellow | AnsiColor::BrightYellow => RgbColor::new(0xff, 0xb3, 0x47),
            AnsiColor::Blue | AnsiColor::BrightBlue => RgbColor::new(0x66, 0x99, 0xff),
            AnsiColor::Magenta | AnsiColor::BrightMagenta => RgbColor::new(0xc0, 0x78, 0xd0),
            AnsiColor::Cyan | AnsiColor::BrightCyan => RgbColor::new(0x4d, 0xd0, 0xe1),
            AnsiColor::White | AnsiColor::BrightWhite => RgbColor::new(0xb0, 0xb3, 0xb9),
        },
        AnstyleColor::Ansi256(index) => {
            if let Some(adapted) = adapt_anstyle_color(AnstyleColor::Ansi256(index), detect_color_level()) {
                return anstyle_color_to_rgb(adapted, fallback);
            }
            fallback
        }
    }
}

pub fn syntect_to_styled_span(style: SyntectStyle, text: impl Into<String>, fallback: RgbColor) -> StyledSpan {
    let anstyle = to_anstyle(style);
    let color = anstyle
        .get_fg_color()
        .and_then(|c| adapt_anstyle_color(c, detect_color_level()))
        .map(|c| anstyle_color_to_rgb(c, fallback))
        .unwrap_or(fallback);
    let effects = anstyle.get_effects();
    StyledSpan {
        text: text.into(),
        color,
        weight: if effects.contains(Effects::BOLD) {
            FontWeight::Bold
        } else {
            FontWeight::Normal
        },
        italic: effects.contains(Effects::ITALIC),
        underline: false,
        href: None,
    }
}

/// Build anstyle Style for a span (for ANSI emission).
pub fn span_anstyle(span: &StyledSpan, level: ColorLevel) -> anstyle::Style {
    let mut style = anstyle::Style::new();
    if level != ColorLevel::None {
        let rgb = AnstyleRgb(span.color.r, span.color.g, span.color.b);
        if let Some(fg) = adapt_anstyle_color(AnstyleColor::Rgb(rgb), level) {
            style = style.fg_color(Some(fg));
        }
    }
    let mut effects = Effects::new();
    if span.weight == FontWeight::Bold {
        effects |= Effects::BOLD;
    }
    if span.italic {
        effects |= Effects::ITALIC;
    }
    if span.underline {
        effects |= Effects::UNDERLINE;
    }
    style.effects(effects)
}
