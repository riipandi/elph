/// Reasoning / thinking level shown in the footer and cycled via Shift+Tab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl ThinkingLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" | "x-high" => Self::Xhigh,
            _ => Self::Off,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Minimal,
            Self::Minimal => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Xhigh,
            Self::Xhigh => Self::Off,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_through_levels() {
        assert_eq!(ThinkingLevel::High.next(), ThinkingLevel::Xhigh);
        assert_eq!(ThinkingLevel::Xhigh.next(), ThinkingLevel::Off);
    }
}
