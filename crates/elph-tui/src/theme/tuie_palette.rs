//! Harmonious tuie palette application for Elph themes.

use std::io;

use tuie::prelude::Border;
use tuie::render::border;
use tuie::theme::Theme as TuieTheme;
use tuie::theme::harmonious::{self, Palette};

use super::palette::{Theme, ThemeMode};

/// Applies a harmonious palette for the active [`Theme`] mode.
///
/// tuie's runtime also refreshes the live terminal palette when the color scheme changes.
pub fn apply_tuie_theme(theme: Theme) -> io::Result<()> {
    let preset = match theme.mode {
        ThemeMode::Dark => TuieTheme::ONE_DARK,
        ThemeMode::Light => TuieTheme::ONE_LIGHT,
    };
    harmonious::apply_palette(Palette::from_theme(preset));
    border::config::update(|cfg| {
        cfg.border = Border::ROUND;
        cfg.selected_border = Border::THICK;
    });
    Ok(())
}
