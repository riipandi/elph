//! Floating prompt-history palette anchored above the editor.

use elph_tui::list_selection_row_prefix;
use iocraft::prelude::*;

use super::model::{PromptHistorySnapshot, entry_preview, history_title};
use crate::tui::slash_palette::palette_window_start;
use crate::tui::slash_palette::row_layout::{palette_card_width, palette_list_width};
use crate::tui::theme::{
    BORDER_MUTED, FILE_PICKER_ROW_IDLE_FG, FILE_PICKER_ROW_SELECTED_BG, FILE_PICKER_ROW_SELECTED_FG, TOOL_ARGS_FG,
};
use crate::types::AgentMode;
use elph_tui::components::theme::UiTheme;

#[derive(Clone, Default, Props)]
pub struct PromptHistoryPaletteProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub agent_mode: AgentMode,
    pub snapshot: PromptHistorySnapshot,
    pub anchor_bottom: u16,
    pub selected_index: Option<State<usize>>,
}

#[component]
pub fn PromptHistoryPalette(props: &PromptHistoryPaletteProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let _ = props.agent_mode;
    if !props.snapshot.should_render() {
        return element! { View(width: 0u16, height: 0u16) {} };
    }

    let theme = UiTheme::default();
    let card_width = palette_card_width(props.screen_width);
    let list_width = palette_list_width(props.screen_width);
    let selected = props.selected_index.as_ref().map(|s| s.get()).unwrap_or(0);
    let len = props.snapshot.entries.len();
    let viewport_rows = props.snapshot.list_height as usize;
    let window_start = palette_window_start(selected, viewport_rows, len);
    let end = window_start.saturating_add(viewport_rows).min(len);
    let visible = &props.snapshot.entries[window_start..end];
    let title = history_title();
    let title_chip = format!(" {title} ");
    let preview_cols = list_width.saturating_sub(3).max(8) as usize;

    element! {
        View(
            width: props.screen_width,
            position: Position::Absolute,
            left: 0,
            bottom: props.anchor_bottom,
            flex_shrink: 0f32,
            align_items: AlignItems::FlexStart,
        ) {
            View(
                width: card_width,
                border_style: BorderStyle::Round,
                border_color: BORDER_MUTED,
                background_color: theme.surface,
                padding_left: 1u16,
                padding_right: 1u16,
                padding_top: 0u16,
                padding_bottom: 0u16,
                flex_direction: FlexDirection::Column,
                position: Position::Relative,
            ) {
                View(
                    position: Position::Absolute,
                    top: 0,
                    left: 1,
                    margin_top: -1,
                    background_color: theme.surface,
                ) {
                    Text(
                        content: title_chip,
                        color: TOOL_ARGS_FG,
                        weight: Weight::Bold,
                        wrap: TextWrap::NoWrap,
                    )
                }
                View(
                    width: list_width,
                    height: props.snapshot.list_height,
                    flex_direction: FlexDirection::Column,
                ) {
                    #(visible.iter().enumerate().map(|(offset, entry)| -> AnyElement<'static> {
                        let row_index = window_start + offset;
                        let selected_row = row_index == selected;
                        let prefix = list_selection_row_prefix(selected_row);
                        let preview = entry_preview(&entry.text, preview_cols);
                        let row_bg = if selected_row {
                            FILE_PICKER_ROW_SELECTED_BG
                        } else {
                            Color::Reset
                        };
                        let fg = if selected_row {
                            FILE_PICKER_ROW_SELECTED_FG
                        } else {
                            FILE_PICKER_ROW_IDLE_FG
                        };
                        element! {
                            View(
                                width: list_width,
                                height: 1,
                                flex_direction: FlexDirection::Row,
                                background_color: row_bg,
                            ) {
                                Text(
                                    content: format!("{prefix}{preview}"),
                                    color: fg,
                                    wrap: TextWrap::NoWrap,
                                )
                            }
                        }
                        .into()
                    }))
                }
            }
        }
    }
}
