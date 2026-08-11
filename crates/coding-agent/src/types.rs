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

    /// Label / border accent color in the TUI (Ghostty dark palette).
    ///
    /// - **Build** palette 7 white · **Plan** palette 3 yellow · **Ask** palette 4 blue · **Brave** warm orange
    pub const fn label_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Build => (0xe0, 0xe2, 0xe8), // palette 7 #e0e2e8
            Self::Plan => (0xff, 0xb3, 0x47),  // palette 3 #ffb347
            Self::Ask => (0x66, 0x99, 0xff),   // palette 4 #6699ff
            Self::Brave => (0xff, 0x8a, 0x4d), // warm orange (between yellow & bright red)
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

/// Reasoning / thinking level (aligned with `elph_ai::ThinkingLevel` + TUI-only `Off`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
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
            Self::Max => "max",
        }
    }

    /// Thinking-level color for footer model group and related chrome.
    ///
    /// Ghostty ANSI strata (distinct from agent-mode accents where possible):
    /// grey → cyan → bright blue → yellow → red → magenta → bright magenta.
    pub const fn border_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Off => (0x7a, 0x7e, 0x85),     // palette 8 #7a7e85
            Self::Minimal => (0x4d, 0xd0, 0xe1), // palette 6 #4dd0e1
            Self::Low => (0x9b, 0xc4, 0xff),     // palette 12 #9bc4ff
            Self::Medium => (0xff, 0xb3, 0x47),  // palette 3 #ffb347
            Self::High => (0xff, 0x6b, 0x66),    // palette 1 #ff6b66
            Self::Xhigh => (0xd4, 0xaa, 0xff),   // palette 5 #d4aaff
            Self::Max => (0xe8, 0xb4, 0xff),     // palette 13 #e8b4ff
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" | "x-high" => Self::Xhigh,
            "max" => Self::Max,
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
            Self::Xhigh => Self::Max,
            Self::Max => Self::Off,
        }
    }

    /// Cycle through Off + levels supported by this model catalog entry.
    ///
    /// Uses [`elph_ai::get_supported_thinking_levels`] so Ctrl+. never lands on a
    /// value the provider API will reject (e.g. `medium` on xAI Grok 4.5).
    pub fn next_for_model(self, model: &elph_ai::Model) -> Self {
        let cycle = Self::cycle_for_model(model);
        if cycle.len() <= 1 {
            return Self::Off;
        }
        // Unsupported current level (stale settings) starts from Off so the next
        // press always lands on a catalog-valid value.
        let start = if cycle.contains(&self) { self } else { Self::Off };
        let idx = cycle.iter().position(|l| *l == start).unwrap_or(0);
        cycle[(idx + 1) % cycle.len()]
    }

    /// Off + catalog-supported levels for this model (footer / switcher source of truth).
    pub fn cycle_for_model(model: &elph_ai::Model) -> Vec<Self> {
        let mut cycle = vec![Self::Off];
        if !model.reasoning {
            return cycle;
        }
        for level in elph_ai::get_supported_thinking_levels(model) {
            cycle.push(Self::from_ai(level));
        }
        cycle
    }

    /// Clamp to Off or a supported catalog level for this model.
    pub fn clamp_for_model(self, model: &elph_ai::Model) -> Self {
        if self == Self::Off || !model.reasoning {
            return Self::Off;
        }
        let ai = elph_ai::clamp_thinking_level(model, self.to_ai());
        Self::from_ai(ai)
    }

    /// Clamp using a builtin catalog lookup (`provider` + `model_id`).
    ///
    /// Unknown models leave the level unchanged so footer/Ctrl+. stay usable offline.
    pub fn clamp_for_provider_model(self, provider: &str, model_id: &str) -> Self {
        elph_ai::get_builtin_model(provider, model_id)
            .map(|model| self.clamp_for_model(&model))
            .unwrap_or(self)
    }

    fn from_ai(level: elph_ai::ThinkingLevel) -> Self {
        match level {
            elph_ai::ThinkingLevel::Minimal => Self::Minimal,
            elph_ai::ThinkingLevel::Low => Self::Low,
            elph_ai::ThinkingLevel::Medium => Self::Medium,
            elph_ai::ThinkingLevel::High => Self::High,
            elph_ai::ThinkingLevel::Xhigh => Self::Xhigh,
            elph_ai::ThinkingLevel::Max => Self::Max,
        }
    }

    fn to_ai(self) -> elph_ai::ThinkingLevel {
        match self {
            Self::Off | Self::Minimal => elph_ai::ThinkingLevel::Minimal,
            Self::Low => elph_ai::ThinkingLevel::Low,
            Self::Medium => elph_ai::ThinkingLevel::Medium,
            Self::High => elph_ai::ThinkingLevel::High,
            Self::Xhigh => elph_ai::ThinkingLevel::Xhigh,
            Self::Max => elph_ai::ThinkingLevel::Max,
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
        assert_eq!(ThinkingLevel::Xhigh.next(), ThinkingLevel::Max);
        assert_eq!(ThinkingLevel::Max.next(), ThinkingLevel::Off);
    }

    #[test]
    fn thinking_next_for_model_respects_catalog_map() {
        let Some(model) = elph_ai::get_builtin_model("xai", "grok-4.5") else {
            return;
        };
        let cycle = ThinkingLevel::cycle_for_model(&model);
        // xAI grok-4.5 supports low / medium / high (from models.dev)
        assert_eq!(
            cycle,
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ]
        );
        // Supported: low, medium, high (+ Off). minimal/xhigh/max must not appear.
        let mut level = ThinkingLevel::Off;
        let mut seen = Vec::new();
        for _ in 0..8 {
            level = level.next_for_model(&model);
            seen.push(level);
            if level == ThinkingLevel::Off && seen.len() > 1 {
                break;
            }
        }
        assert!(seen.contains(&ThinkingLevel::Low));
        assert!(seen.contains(&ThinkingLevel::High));
        assert!(seen.contains(&ThinkingLevel::Medium));
        assert!(!seen.contains(&ThinkingLevel::Max));
        assert!(!seen.contains(&ThinkingLevel::Minimal));
        assert!(!seen.contains(&ThinkingLevel::Xhigh));
        // Medium is natively supported — cycles to High, clamps to itself.
        assert_eq!(ThinkingLevel::Medium.next_for_model(&model), ThinkingLevel::High);
        assert_eq!(ThinkingLevel::Medium.clamp_for_model(&model), ThinkingLevel::Medium);
    }

    #[test]
    fn thinking_level_from_setting_accepts_max_and_xhigh() {
        assert_eq!(ThinkingLevel::from_setting("max"), ThinkingLevel::Max);
        assert_eq!(ThinkingLevel::from_setting("xhigh"), ThinkingLevel::Xhigh);
        assert_eq!(ThinkingLevel::from_setting("x-high"), ThinkingLevel::Xhigh);
        assert_eq!(ThinkingLevel::Max.label(), "max");
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

/// Classification for Pi-style `/tree` filter modes (and generic pickers).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectItemKind {
    #[default]
    Generic,
    UserMessage,
    AssistantMessage,
    /// Tool result / tool-role message (hidden by default and `no-tools`).
    ToolResult,
    BranchSummary,
    Compaction,
    /// Explicit label entry or a message that has a label attached.
    Label,
    /// Bookkeeping: model/thinking/session_info/custom settings (hidden by default).
    Settings,
}

/// One selectable row (previously in elph-tui diff module).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
    /// Tree/filter classification (default [`SelectItemKind::Generic`]).
    pub kind: SelectItemKind,
    /// True when a session label points at this entry (Pi `labeled-only`).
    pub labeled: bool,
}

impl SelectItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
            kind: SelectItemKind::Generic,
            labeled: false,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_kind(mut self, kind: SelectItemKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_labeled(mut self, labeled: bool) -> Self {
        self.labeled = labeled;
        self
    }
}

/// Classifies the origin of a slash command for display and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandKind {
    /// Built-in command such as `/help`, `/model`, `/exit`.
    Builtin,
    /// Extension command registered by an extension host.
    Extension,
    /// Skill invoked as `/skill:<name>`.
    Skill,
    /// Loaded prompt template such as `/review`.
    PromptTemplate,
}

/// Slash command entry for prompt autocomplete (previously in elph-tui diff module).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub args_hint: Option<String>,
    /// When true, omitted from the default `/` palette list and `/help`, but still
    /// dispatchable when typed and completable via Tab once the query matches.
    pub hidden: bool,
    pub kind: SlashCommandKind,
}

impl SlashCommand {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            args_hint: None,
            hidden: false,
            kind: SlashCommandKind::Builtin,
        }
    }

    pub fn with_args_hint(mut self, hint: impl Into<String>) -> Self {
        self.args_hint = Some(hint.into());
        self
    }

    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    pub fn with_kind(mut self, kind: SlashCommandKind) -> Self {
        self.kind = kind;
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
