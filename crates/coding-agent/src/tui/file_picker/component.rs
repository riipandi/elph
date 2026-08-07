//! Floating `@` file picker anchored above the editor.

use elph_tui::list_selection_row_prefix;
use iocraft::prelude::*;

use super::fuzzy_highlight::file_picker_row_runs;
use super::model::{FilePickerSnapshot, file_picker_title};
use crate::tui::slash_palette::palette_window_start;
use crate::tui::slash_palette::row_layout::palette_list_width;
use crate::tui::theme::{
    BORDER_MUTED, FILE_PICKER_FUZZY_MATCH_FG, FILE_PICKER_ROW_IDLE_FG, FILE_PICKER_ROW_SELECTED_BG,
    FILE_PICKER_ROW_SELECTED_FG, TOOL_ARGS_FG,
};
use crate::types::AgentMode;

#[derive(Clone, Default, Props)]
pub struct FilePickerPaletteProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub agent_mode: AgentMode,
    pub snapshot: FilePickerSnapshot,
    pub anchor_bottom: u16,
    pub selected_index: Option<State<usize>>,
    pub show_hidden_files: bool,
}

#[component]
pub fn FilePickerPalette(props: &FilePickerPaletteProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    if !props.snapshot.should_render() {
        return element! { View(width: 0u16, height: 0u16) {} };
    }

    let list_width = palette_list_width(props.screen_width);
    let selected = props.selected_index.as_ref().map(|index| index.get()).unwrap_or(0);
    let len = props.snapshot.options.len();
    let viewport_rows = props.snapshot.list_height as usize;
    let window_start = palette_window_start(selected, viewport_rows, len);
    let visible = props.snapshot.options[window_start..window_start.saturating_add(viewport_rows).min(len)].to_vec();

    let title = file_picker_title(
        &props.snapshot.query,
        props.snapshot.file_count,
        props.snapshot.dir_count,
        props.show_hidden_files,
    );

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
                width: props.screen_width,
                border_style: BorderStyle::Round,
                border_color: BORDER_MUTED,
                flex_direction: FlexDirection::Column,
                padding_left: 1,
                padding_right: 1,
                padding_top: 0,
                padding_bottom: 0,
            ) {
                Text(
                    content: title,
                    color: TOOL_ARGS_FG,
                    weight: Weight::Normal,
                    wrap: TextWrap::NoWrap,
                )
                View(
                    width: list_width,
                    height: props.snapshot.list_height,
                    flex_direction: FlexDirection::Column,
                ) {
                    #(visible.into_iter().enumerate().map(|(offset, option)| -> AnyElement<'static> {
                        let row_index = window_start + offset;
                        let selected_row = row_index == selected;
                        let prefix = list_selection_row_prefix(selected_row);
                        let row_bg = if selected_row {
                            FILE_PICKER_ROW_SELECTED_BG
                        } else {
                            Color::Reset
                        };
                        let base_fg = if selected_row {
                            FILE_PICKER_ROW_SELECTED_FG
                        } else {
                            FILE_PICKER_ROW_IDLE_FG
                        };
                        let runs = file_picker_row_runs(prefix, &option.path, &props.snapshot.query);
                        element! {
                            View(
                                width: list_width,
                                height: 1,
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::FlexStart,
                                background_color: row_bg,
                            ) {
                                #(runs.into_iter().map(|run| -> AnyElement<'static> {
                                    let color = if run.matched { FILE_PICKER_FUZZY_MATCH_FG } else { base_fg };
                                    element! {
                                        Text(
                                            content: run.text,
                                            color: color,
                                            weight: Weight::Normal,
                                            wrap: TextWrap::NoWrap,
                                        )
                                    }
                                    .into()
                                }))
                            }
                        }
                        .into()
                    }))
                }
            }
        }
    }
}
