mod palette;
mod tuie_palette;

pub use palette::{Theme, ThemeMode};
pub use tuie::prelude::{Border, Color};
pub use tuie_palette::apply_tuie_theme;

/// Rounded box-drawing border used for shell panels and popups.
pub fn shell_border() -> &'static Border {
    Border::ROUND
}
