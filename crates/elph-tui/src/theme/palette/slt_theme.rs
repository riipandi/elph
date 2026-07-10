use slt::{Color, Context, Theme as SltTheme};

use super::{Theme, ThemeMode};

/// Sync SuperLightTUI with a terminal-respecting palette for the active mode.
pub fn apply_slt_theme(ui: &mut Context, theme: Theme) {
    ui.set_theme(slt_theme(theme));
}

fn slt_theme(theme: Theme) -> SltTheme {
    match theme.mode {
        ThemeMode::Dark => SltTheme::builder()
            .is_dark(true)
            .text(Color::Reset)
            .text_dim(Color::DarkGray)
            .bg(Color::Reset)
            .border(Color::DarkGray)
            .primary(Color::Cyan)
            .secondary(Color::Blue)
            .accent(Color::Magenta)
            .success(Color::Green)
            .warning(Color::Yellow)
            .error(Color::Red)
            .selected_bg(Color::Blue)
            .selected_fg(Color::Reset)
            .surface(Color::Reset)
            .surface_hover(Color::Reset)
            .surface_text(Color::Reset)
            .build(),
        ThemeMode::Light => SltTheme::builder()
            .is_dark(false)
            .text(Color::Reset)
            .text_dim(Color::DarkGray)
            .bg(Color::Reset)
            .border(Color::DarkGray)
            .primary(Color::Blue)
            .secondary(Color::Cyan)
            .accent(Color::Magenta)
            .success(Color::Green)
            .warning(Color::Yellow)
            .error(Color::Red)
            .selected_bg(Color::Blue)
            .selected_fg(Color::Reset)
            .surface(Color::Reset)
            .surface_hover(Color::Reset)
            .surface_text(Color::Reset)
            .build(),
    }
}
