//! Resolved chrome tokens for one palette card render.

use iocraft::prelude::Color;

use super::super::model::SlashPaletteSnapshot;
use super::super::row_layout;
use crate::tui::theme::{BORDER_MUTED, TEXT_FG, TOOL_ARGS_FG};
use crate::types::AgentMode;

/// Precomputed layout and copy for [`super::SlashPaletteCard`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteCardChrome {
    pub card_width: u16,
    pub content_width: u16,
    pub list_width: u16,
    pub border_color: Color,
    pub background: Color,
    pub title_color: Color,
    pub name_idle_color: Color,
    pub name_active_color: Color,
    pub desc_active_color: Color,
    pub desc_idle_color: Color,
    pub title: String,
}

impl Default for PaletteCardChrome {
    fn default() -> Self {
        Self {
            card_width: 0,
            content_width: 0,
            list_width: 0,
            border_color: Color::Reset,
            background: Color::Reset,
            title_color: Color::Reset,
            name_idle_color: Color::Reset,
            name_active_color: Color::Reset,
            desc_active_color: Color::Reset,
            desc_idle_color: Color::Reset,
            title: String::new(),
        }
    }
}

impl PaletteCardChrome {
    pub fn from_snapshot(screen_width: u16, _agent_mode: AgentMode, snapshot: &SlashPaletteSnapshot) -> Self {
        let card_width = row_layout::palette_card_width(screen_width);
        let list_width = row_layout::palette_list_width(screen_width);
        let content_width = list_width;
        Self {
            card_width,
            content_width,
            list_width,
            border_color: BORDER_MUTED,
            background: Color::Reset,
            title_color: TOOL_ARGS_FG,
            name_idle_color: TEXT_FG,
            name_active_color: TEXT_FG,
            desc_active_color: TEXT_FG,
            desc_idle_color: Color::DarkGrey,
            title: card_title(snapshot),
        }
    }
}

fn card_title(snapshot: &SlashPaletteSnapshot) -> String {
    format!("{:02} Commands", snapshot.match_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentMode, SlashCommand};

    use super::super::super::model::build_snapshot;

    #[test]
    fn title_merges_count_with_commands_label() {
        let commands = vec![
            SlashCommand::new("help", "List commands"),
            SlashCommand::new("goal", "Goals"),
        ];
        let snapshot = build_snapshot("/", &commands, 40);
        let chrome = PaletteCardChrome::from_snapshot(80, AgentMode::Build, &snapshot);
        assert_eq!(chrome.title, "02 Commands");
    }

    #[test]
    fn title_stays_static_while_filtering() {
        let commands = vec![SlashCommand::new("help", "List commands")];
        let snapshot = build_snapshot("/he", &commands, 40);
        let chrome = PaletteCardChrome::from_snapshot(80, AgentMode::Build, &snapshot);
        assert_eq!(chrome.title, "01 Commands");
    }

    #[test]
    fn widths_derive_from_screen_width() {
        let snapshot = SlashPaletteSnapshot::hidden();
        let chrome = PaletteCardChrome::from_snapshot(80, AgentMode::Build, &snapshot);
        assert_eq!(chrome.card_width, 80);
        assert_eq!(chrome.list_width, 77);
        assert!(chrome.list_width <= chrome.card_width);
    }
}
