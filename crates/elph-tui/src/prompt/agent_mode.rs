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

    /// Lowercase label for the footer status line.
    pub fn footer_label(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Plan => "plan",
            Self::Ask => "ask",
            Self::Brave => "brave",
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle() {
        assert_eq!(AgentMode::Build.next(), AgentMode::Plan);
        assert_eq!(AgentMode::Brave.next(), AgentMode::Build);
    }
}
