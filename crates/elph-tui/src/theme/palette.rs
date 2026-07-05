use crate::prompt::AgentMode;
use iocraft::prelude::Color;

/// Visual theme variant for the terminal UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

/// Color palette applied across TUI components.
///
/// Base colors use [`Color::Reset`] so foreground and background inherit the
/// terminal's configured theme. Accent and muted colors use ANSI names so they
/// map to the terminal palette instead of fixed RGB values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub mode: ThemeMode,
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
    pub prompt_prefix: Color,
    pub scrollbar_thumb: Color,
    pub scrollbar_track: Color,
    pub frame_border: Color,
    mode_build: Color,
    mode_plan: Color,
    mode_ask: Color,
    mode_brave: Color,
}

impl Theme {
    /// Palette tuned for dark terminal backgrounds.
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            background: Color::Reset,
            foreground: Color::Reset,
            muted: Color::Grey,
            prompt_prefix: Color::DarkGrey,
            scrollbar_thumb: Color::Grey,
            scrollbar_track: Color::DarkGrey,
            frame_border: Color::Blue,
            mode_build: Color::DarkGrey,
            mode_plan: Color::Green,
            mode_ask: Color::Blue,
            mode_brave: Color::Red,
        }
    }

    /// Palette tuned for light terminal backgrounds.
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            background: Color::Reset,
            foreground: Color::Reset,
            muted: Color::DarkGrey,
            prompt_prefix: Color::Grey,
            scrollbar_thumb: Color::DarkGrey,
            scrollbar_track: Color::Grey,
            frame_border: Color::DarkBlue,
            mode_build: Color::DarkGrey,
            mode_plan: Color::DarkGreen,
            mode_ask: Color::DarkBlue,
            mode_brave: Color::DarkRed,
        }
    }

    pub fn from_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
        }
    }

    /// Resolves the active theme from `ELPH_THEME`, terminal `COLORFGBG`, or defaults to dark.
    pub fn detect() -> Self {
        if let Ok(value) = std::env::var("ELPH_THEME") {
            match value.trim().to_ascii_lowercase().as_str() {
                "light" => return Self::light(),
                "dark" => return Self::dark(),
                _ => {}
            }
        }

        if let Ok(fgbg) = std::env::var("COLORFGBG")
            && let Some(bg) = fgbg.split(';').nth(1).and_then(|part| part.trim().parse::<u8>().ok())
        {
            return if bg >= 8 { Self::light() } else { Self::dark() };
        }

        Self::dark()
    }

    pub fn toggle(self) -> Self {
        Self::from_mode(match self.mode {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        })
    }

    pub fn mode_accent(self, mode: AgentMode) -> Color {
        match mode {
            AgentMode::Build => self.mode_build,
            AgentMode::Plan => self.mode_plan,
            AgentMode::Ask => self.mode_ask,
            AgentMode::Brave => self.mode_brave,
        }
    }

    /// Background color for views; `None` leaves the terminal background untouched.
    pub fn view_background(self) -> Option<Color> {
        match self.background {
            Color::Reset => None,
            color => Some(color),
        }
    }

    /// Foreground color for text; `None` inherits the terminal foreground.
    pub fn text_color(self) -> Option<Color> {
        match self.foreground {
            Color::Reset => None,
            color => Some(color),
        }
    }

    /// Block cursor color for the prompt field.
    pub fn input_cursor(self) -> Color {
        match self.mode {
            ThemeMode::Dark => Color::Grey,
            ThemeMode::Light => Color::DarkGrey,
        }
    }

    /// Placeholder hint shown when the prompt is empty.
    pub fn input_placeholder(self) -> Color {
        match self.mode {
            ThemeMode::Dark => Color::DarkGrey,
            ThemeMode::Light => Color::Grey,
        }
    }

    /// Label color for collapsed paste chips (`[Pasted: NN lines]`).
    pub fn paste_label(self) -> Color {
        self.muted
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_palettes_differ() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert_eq!(dark.background, Color::Reset);
        assert_eq!(light.background, Color::Reset);
        assert_eq!(dark.foreground, Color::Reset);
        assert_eq!(light.foreground, Color::Reset);
        assert_ne!(dark.muted, light.muted);
        assert_ne!(dark.frame_border, light.frame_border);
        assert_eq!(dark.mode, ThemeMode::Dark);
        assert_eq!(light.mode, ThemeMode::Light);
    }

    #[test]
    fn toggle_switches_mode() {
        let dark = Theme::dark();
        assert_eq!(dark.toggle().mode, ThemeMode::Light);
        assert_eq!(dark.toggle().toggle().mode, ThemeMode::Dark);
    }

    #[test]
    fn mode_accent_returns_palette_entry() {
        let theme = Theme::dark();
        assert_eq!(theme.mode_accent(AgentMode::Plan), Color::Green);
    }

    #[test]
    fn palettes_use_ansi_not_rgb() {
        let theme = Theme::dark();
        assert!(!matches!(theme.muted, Color::Rgb { .. } | Color::AnsiValue(_)));
    }

    #[test]
    fn reset_colors_defer_to_terminal() {
        let theme = Theme::dark();
        assert_eq!(theme.view_background(), None);
        assert_eq!(theme.text_color(), None);
    }
}
