//! Shared types for interactive components.

use iocraft::prelude::Color;
use serde::{Deserialize, Serialize};

/// One selectable row (OpenTUI Select option).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectOption {
    pub name: String,
    pub description: String,
    /// Optional per-row description color. When set, the [`SelectList`][crate::components::SelectList]
    /// uses this instead of the default theme-based description color.
    #[serde(skip)]
    pub description_color: Option<Color>,
}

impl SelectOption {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            description_color: None,
        }
    }
}

/// One tab in a tab selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabItem {
    pub label: String,
    pub content: String,
}

impl TabItem {
    pub fn new(label: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            content: content.into(),
        }
    }
}

/// Status glyph for a static dialog todo row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogTodoStatus {
    Pending,
    Done,
    Skipped,
}

/// Progress state for an in-flight dialog todo row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogTodoProgress {
    Queued,
    Running,
    Done,
    Failed,
}

/// One checklist row inside [`crate::components::DialogTodoListContent`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogTodoItem {
    pub label: String,
    pub detail: String,
    pub status: DialogTodoStatus,
}

impl DialogTodoItem {
    pub fn new(label: impl Into<String>, status: DialogTodoStatus) -> Self {
        Self {
            label: label.into(),
            detail: String::new(),
            status,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }
}

/// One progress row inside [`crate::components::DialogTodoProgressContent`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogTodoProgressItem {
    pub label: String,
    pub state: DialogTodoProgress,
}

impl DialogTodoProgressItem {
    pub fn new(label: impl Into<String>, state: DialogTodoProgress) -> Self {
        Self {
            label: label.into(),
            state,
        }
    }
}

/// Agent interaction mode for dialog demos and future elph wiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogAgentMode {
    #[default]
    Build,
    Plan,
    Ask,
    Brave,
}

impl DialogAgentMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Plan => "plan",
            Self::Ask => "ask",
            Self::Brave => "brave",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Build => "Full tool access for implementation work",
            Self::Plan => "Read-only exploration and planning",
            Self::Ask => "Q&A without mutating tools",
            Self::Brave => "Elevated permissions for risky operations",
        }
    }

    pub fn accent_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Plan => (6, 182, 212),
            Self::Ask => (59, 130, 246),
            Self::Brave => (239, 68, 68),
            Self::Build => (107, 114, 128),
        }
    }

    pub fn all() -> [Self; 4] {
        [Self::Build, Self::Plan, Self::Ask, Self::Brave]
    }
}
