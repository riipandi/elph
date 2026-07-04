use iocraft::prelude::Color;

/// Agent permission / interaction mode shown in the prompt footer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentMode {
    #[default]
    Build,
    Plan,
    Ask,
    Brave,
}

impl AgentMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Build => "Build",
            Self::Plan => "Plan",
            Self::Ask => "Ask",
            Self::Brave => "Brave",
        }
    }

    /// Accent for the prompt border and mode label only.
    pub fn accent_color(self) -> Color {
        match self {
            Self::Build => rgb(0x6B, 0x72, 0x80),
            Self::Plan => rgb(0x06, 0xB6, 0xD4),
            Self::Ask => rgb(0x3B, 0x82, 0xF6),
            Self::Brave => rgb(0xEF, 0x44, 0x44),
        }
    }

    /// Alias for [`Self::accent_color`].
    pub fn border_color(self) -> Color {
        self.accent_color()
    }

    /// Neutral tone for static footer text (e.g. model name).
    pub fn status_muted_color() -> Color {
        rgb(0x9C, 0xA3, 0xAF)
    }

    pub fn next(self) -> Self {
        match self {
            Self::Build => Self::Plan,
            Self::Plan => Self::Ask,
            Self::Ask => Self::Brave,
            Self::Brave => Self::Build,
        }
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb { r, g, b }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle() {
        assert_eq!(AgentMode::Build.next(), AgentMode::Plan);
        assert_eq!(AgentMode::Brave.next(), AgentMode::Build);
    }
}
