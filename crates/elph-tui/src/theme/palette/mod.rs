mod detect;
mod semantic;

use crate::prompt::AgentMode;
use tuie::prelude::Color;

/// Visual theme variant for the terminal UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

/// Terminal-native palette — no custom RGB; defers to the emulator theme.
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
    pub(super) mode_build: Color,
    pub(super) mode_plan: Color,
    pub(super) mode_ask: Color,
    pub(super) mode_brave: Color,
}

impl Theme {
    /// Palette for dark terminal backgrounds (standard ANSI accents).
    pub fn dark() -> Self {
        Self::from_mode(ThemeMode::Dark)
    }

    /// Palette for light terminal backgrounds (standard ANSI accents).
    pub fn light() -> Self {
        Self::from_mode(ThemeMode::Light)
    }

    pub fn from_mode(mode: ThemeMode) -> Self {
        Self {
            mode,
            background: Color::Background,
            foreground: Color::Foreground,
            muted: Color::grey256(8),
            prompt_prefix: Color::Foreground,
            scrollbar_thumb: Color::grey256(8),
            scrollbar_track: Color::grey256(8),
            frame_border: Color::Foreground,
            mode_build: Color::grey256(8),
            mode_plan: Color::CYAN,
            mode_ask: Color::BLUE,
            mode_brave: Color::RED,
        }
    }

    /// Resolves the active theme from `ELPH_THEME`, terminal `COLORFGBG`, or defaults to dark.
    pub fn detect() -> Self {
        detect::detect()
    }

    pub fn toggle(self) -> Self {
        Self::from_mode(match self.mode {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        })
    }

    /// Sync tuie with a terminal-respecting palette for the active mode.
    pub fn apply_tuie_theme(self) -> std::io::Result<()> {
        crate::theme::tuie_palette::apply_tuie_theme(self)
    }

    pub fn mode_accent(self, mode: AgentMode) -> Color {
        self.mode_border_color(mode)
    }

    pub fn mode_border_color(self, mode: AgentMode) -> Color {
        match mode {
            AgentMode::Build => self.mode_build,
            AgentMode::Plan => self.mode_plan,
            AgentMode::Ask => self.mode_ask,
            AgentMode::Brave => self.mode_brave,
        }
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
    fn palettes_use_terminal_defaults() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert_eq!(dark.background, Color::Background);
        assert_eq!(light.background, Color::Background);
        assert_eq!(dark.foreground, Color::Foreground);
        assert_eq!(light.foreground, Color::Foreground);
        assert_eq!(dark.mode, ThemeMode::Dark);
        assert_eq!(light.mode, ThemeMode::Light);
    }

    #[test]
    fn no_custom_rgb_in_semantic_tokens() {
        let theme = Theme::dark();
        for color in [
            theme.blue_col(),
            theme.yellow_col(),
            theme.highlight(),
            theme.special(),
            theme.dim_text(),
            theme.bright_text(),
            theme.user_pipe_col(),
            theme.ai_pipe_col(),
            theme.white_col(),
            theme.thinking_color("high"),
            theme.context_usage_color(95.0),
            theme.git_status_color(1, 0),
            theme.mode_border_color(AgentMode::Ask),
        ] {
            assert!(!matches!(color, Color::Rgb(_, _, _)), "expected terminal color, got {color:?}");
        }
    }

    #[test]
    fn toggle_switches_mode() {
        let dark = Theme::dark();
        assert_eq!(dark.toggle().mode, ThemeMode::Light);
        assert_eq!(dark.toggle().toggle().mode, ThemeMode::Dark);
    }

    #[test]
    fn mode_border_uses_ansi() {
        let theme = Theme::dark();
        assert_eq!(theme.mode_border_color(AgentMode::Plan), Color::CYAN);
        assert_eq!(theme.mode_border_color(AgentMode::Ask), Color::BLUE);
    }

    #[test]
    fn context_usage_thresholds() {
        let theme = Theme::dark();
        assert_eq!(theme.context_usage_color(30.0), Color::Foreground);
        assert_eq!(theme.context_usage_color(60.0), Color::YELLOW);
        assert_eq!(theme.context_usage_color(85.0), Color::BRIGHT_RED);
        assert_eq!(theme.context_usage_color(95.0), Color::RED);
    }

    #[test]
    fn reset_colors_defer_to_terminal() {
        let theme = Theme::dark();
        assert_eq!(theme.view_background(), None);
        assert_eq!(theme.text_color(), None);
    }
}
