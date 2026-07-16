//! Flat command list inside the palette card.

use iocraft::prelude::*;

use super::super::model::SlashPaletteSnapshot;
use super::super::model::{list_viewport_cap, palette_window_start};
use super::super::row_layout::CMD_DESC_GAP_COLS;
use super::super::row_layout::{visible_terminal_rows, wrap_palette_description};
use super::chrome::PaletteCardChrome;
use elph_tui::list_selection_row_prefix;

#[derive(Clone, Default, Props)]
pub struct PaletteCardBodyProps {
    pub chrome: PaletteCardChrome,
    pub snapshot: SlashPaletteSnapshot,
    pub selected_index: Option<State<usize>>,
    pub screen_height: u16,
}

fn palette_command_row(
    chrome: &PaletteCardChrome,
    command_name: &str,
    args_hint: Option<&str>,
    description: &str,
    selected: bool,
) -> AnyElement<'static> {
    let prefix = list_selection_row_prefix(selected);
    let name_color = if selected {
        chrome.name_active_color
    } else {
        chrome.name_idle_color
    };
    let name_weight = if selected { Weight::Bold } else { Weight::Normal };
    let desc_color = if selected {
        chrome.desc_active_color
    } else {
        chrome.desc_idle_color
    };

    let cmd_col = chrome.command_column_width;
    let desc_width = chrome.list_width.saturating_sub(cmd_col + CMD_DESC_GAP_COLS).max(1);
    let desc_lines = wrap_palette_description(description, chrome.list_width, cmd_col);
    let row_height = desc_lines.len().max(1) as u16;
    let desc_text = desc_lines.join("\n");

    let mut name_segments: Vec<AnyElement<'static>> = Vec::new();
    name_segments.push(
        element! {
            Text(
                content: format!("{prefix}{command_name}"),
                color: name_color,
                weight: name_weight,
                wrap: TextWrap::NoWrap,
                align: TextAlign::Left,
            )
        }
        .into(),
    );
    if let Some(hint) = args_hint {
        name_segments.push(
            element! {
                Text(
                    content: format!(" {hint}"),
                    color: chrome.args_hint_color,
                    weight: Weight::Normal,
                    wrap: TextWrap::NoWrap,
                    align: TextAlign::Left,
                )
            }
            .into(),
        );
    }

    element! {
        View(
            width: chrome.list_width,
            height: row_height,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            justify_content: JustifyContent::FlexStart,
            gap: CMD_DESC_GAP_COLS,
        ) {
            View(width: cmd_col, height: row_height, align_items: AlignItems::FlexStart, flex_direction: FlexDirection::Row) {
                #(name_segments)
            }
            View(width: desc_width, height: row_height, align_items: AlignItems::FlexStart) {
                Text(
                    content: desc_text,
                    color: desc_color,
                    wrap: TextWrap::NoWrap,
                    align: TextAlign::Left,
                )
            }
        }
    }
    .into()
}

fn palette_arg_row(chrome: &PaletteCardChrome, arg: &str, description: &str, selected: bool) -> AnyElement<'static> {
    palette_command_row(chrome, arg, None, description, selected)
}

fn palette_list_rows(props: &PaletteCardBodyProps) -> Vec<AnyElement<'static>> {
    let options = &props.snapshot.options;
    let len = options.len();
    let viewport_cap = list_viewport_cap(props.screen_height).max(1);
    let index = props
        .selected_index
        .map(|state| state.get())
        .unwrap_or(0)
        .min(len.saturating_sub(1));
    let scroll_cap = viewport_cap.min(len.max(1));
    let window_start = palette_window_start(index, scroll_cap, len);

    if props.snapshot.is_args_phase() {
        return options
            .iter()
            .enumerate()
            .skip(window_start)
            .take(scroll_cap)
            .map(|(i, opt)| palette_arg_row(&props.chrome, &opt.name, &opt.description, i == index))
            .collect();
    }

    options
        .iter()
        .enumerate()
        .skip(window_start)
        .take(scroll_cap)
        .zip(
            props
                .snapshot
                .filtered_commands
                .iter()
                .skip(window_start)
                .take(scroll_cap),
        )
        .map(|((i, opt), cmd)| {
            palette_command_row(&props.chrome, &opt.name, cmd.args_hint.as_deref(), &opt.description, i == index)
        })
        .collect()
}

#[component]
pub fn PaletteCardBody(props: &PaletteCardBodyProps) -> impl Into<AnyElement<'static>> {
    let fixed_height = props.snapshot.list_height;

    if props.snapshot.has_matches() {
        let rows = palette_list_rows(props);
        let options = &props.snapshot.options;
        let len = options.len();
        let viewport_cap = list_viewport_cap(props.screen_height).max(1);
        let index = props
            .selected_index
            .map(|state| state.get())
            .unwrap_or(0)
            .min(len.saturating_sub(1));
        let scroll_cap = viewport_cap.min(len.max(1));
        let window_start = palette_window_start(index, scroll_cap, len);
        let body_height = visible_terminal_rows(
            options,
            window_start,
            scroll_cap,
            props.chrome.list_width,
            props.chrome.command_column_width,
            viewport_cap,
        );
        element! {
            View(
                width: props.chrome.list_width,
                height: body_height,
                flex_direction: FlexDirection::Column,
                gap: 0,
                align_items: AlignItems::FlexStart,
            ) {
                #(rows)
            }
        }
    } else {
        element! {
            View(
                width: props.chrome.list_width,
                height: fixed_height,
                align_items: AlignItems::FlexStart,
                justify_content: JustifyContent::Center,
            ) {
                Text(
                    content: "No matches",
                    color: props.chrome.desc_idle_color,
                    wrap: TextWrap::NoWrap,
                )
            }
        }
    }
}
