//! Shared UI and session types for the Elph binary.

/// Agent permission / interaction mode.
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

    pub fn footer_label(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Plan => "plan",
            Self::Ask => "ask",
            Self::Brave => "brave",
        }
    }

    /// Label color in the TUI (see `docs/tui.md` agent mode palette).
    pub const fn label_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Plan => (6, 182, 212),
            Self::Ask => (59, 130, 246),
            Self::Brave => (239, 68, 68),
            Self::Build => (107, 114, 128),
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

/// Reasoning / thinking level.
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

    /// Editor border color in the TUI (see `docs/tui.md` thinking level palette).
    pub const fn border_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Off | Self::Minimal => (107, 114, 128),
            Self::Low => (34, 197, 94),
            Self::Medium => (234, 179, 8),
            Self::High => (249, 115, 22),
            Self::Xhigh => (239, 68, 68),
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

/// Actions the prompt can signal to the parent app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAction {
    None,
    Submit(String),
    Queue(String),
    Steer(String),
    Clear,
    CycleMode,
}

/// Returns true when submitted text is the Neovim-style quit command (`:q`).
pub fn is_quit_command(text: &str) -> bool {
    text.trim() == ":q"
}

/// Returns true for forced quit (`:q!`) — exits immediately, even during an active turn.
pub fn is_force_quit_command(text: &str) -> bool {
    text.trim() == ":q!"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle() {
        assert_eq!(AgentMode::Build.next(), AgentMode::Plan);
        assert_eq!(AgentMode::Brave.next(), AgentMode::Build);
    }

    #[test]
    fn thinking_levels_cycle() {
        assert_eq!(ThinkingLevel::High.next(), ThinkingLevel::Xhigh);
        assert_eq!(ThinkingLevel::Xhigh.next(), ThinkingLevel::Off);
    }

    #[test]
    fn detects_quit_command() {
        assert!(is_quit_command(":q"));
        assert!(!is_quit_command(":q!"));
        assert!(!is_quit_command("hello"));
    }

    #[test]
    fn detects_force_quit_command() {
        assert!(is_force_quit_command(":q!"));
        assert!(!is_force_quit_command(":q"));
    }
}

/// One selectable row (previously in elph-tui diff module).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl SelectItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Slash command entry for prompt autocomplete (previously in elph-tui diff module).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub args_hint: Option<String>,
}

impl SlashCommand {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            args_hint: None,
        }
    }

    pub fn with_args_hint(mut self, hint: impl Into<String>) -> Self {
        self.args_hint = Some(hint.into());
        self
    }

    pub fn palette_command_name(&self) -> String {
        format!("/{}", self.name)
    }

    pub fn palette_command_label(&self) -> String {
        match &self.args_hint {
            Some(hint) => format!("{} {hint}", self.palette_command_name()),
            None => self.palette_command_name(),
        }
    }
}
