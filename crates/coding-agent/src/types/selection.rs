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
