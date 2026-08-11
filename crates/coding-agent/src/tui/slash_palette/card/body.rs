//! Flat command list inside the palette card.

use elph_tui::components::theme::{UiTheme, dialog_option_desc_style, dialog_option_name_style, dialog_row_surface};
use elph_tui::list_selection_row_prefix;
use elph_tui::utils::display_width;
use iocraft::prelude::*;

use super::super::model::SlashPaletteSnapshot;
use super::super::model::{list_viewport_cap, palette_window_start};
use super::super::row_layout::CMD_DESC_GAP_COLS;
use super::super::row_layout::{
    ROW_PREFIX_CHARS, palette_desc_width, truncate_command_label, visible_terminal_rows, wrap_palette_description,
};
use super::chrome::PaletteCardChrome;

#[derive(Clone, Default, Props)]
pub struct PaletteCardBodyProps {
    pub chrome: PaletteCardChrome,
    pub snapshot: SlashPaletteSnapshot,
    pub selected_index: Option<State<usize>>,
    pub screen_height: u16,
    pub theme: Option<UiTheme>,
}

fn palette_command_row(
    chrome: &PaletteCardChrome,
    theme: UiTheme,
    command_name: &str,
    args_hint: Option<&str>,
    description: &str,
    selected: bool,
) -> AnyElement<'static> {
    let prefix = list_selection_row_prefix(selected);
    let (name_color, name_weight) = dialog_option_name_style(theme, selected);
    let desc_color = dialog_option_desc_style(theme, selected);
    let row_surface = dialog_row_surface(theme, selected);

    let cmd_col = chrome.command_column_width;
    let desc_width = palette_desc_width(chrome.list_width, cmd_col);
    let content_max = (cmd_col as usize).saturating_sub(ROW_PREFIX_CHARS);
    let (display_name, display_hint) = truncate_command_label(command_name, args_hint, content_max);
    let desc_text = wrap_palette_description(description, desc_width).join("");

    let mut name_segments: Vec<AnyElement<'static>> = Vec::new();
    name_segments.push(
        element! {
            Text(
                content: format!("{prefix}{display_name}"),
                color: name_color,
                weight: name_weight,
                wrap: TextWrap::NoWrap,
                align: TextAlign::Left,
            )
        }
        .into(),
    );
    if let Some(hint) = display_hint {
        let hint_content = format!(" {hint}");
        let hint_width = display_width(&hint_content) as u16;
        name_segments.push(
            element! {
                View(
                    width: hint_width,
                    height: 1u16,
                    overflow: Overflow::Hidden,
                    flex_shrink: 0f32,
                ) {
                    Text(
                        content: hint_content,
                        color: if selected { theme.text_muted } else { chrome.args_hint_color },
                        weight: Weight::Normal,
                        wrap: TextWrap::NoWrap,
                        align: TextAlign::Left,
                    )
                }
            }
            .into(),
        );
    }

    element! {
        View(
            width: chrome.list_width,
            height: 1u16,
            background_color: row_surface,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            justify_content: JustifyContent::FlexStart,
            gap: CMD_DESC_GAP_COLS,
            flex_shrink: 0f32,
            overflow: Overflow::Hidden,
        ) {
            View(
                width: cmd_col,
                height: 1u16,
                align_items: AlignItems::FlexStart,
                flex_direction: FlexDirection::Row,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
                #(name_segments)
            }
            View(
                width: desc_width as u16,
                height: 1u16,
                align_items: AlignItems::FlexStart,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
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

fn palette_arg_row(
    chrome: &PaletteCardChrome,
    theme: UiTheme,
    arg: &str,
    description: &str,
    selected: bool,
) -> AnyElement<'static> {
    palette_command_row(chrome, theme, arg, None, description, selected)
}

fn palette_list_rows(props: &PaletteCardBodyProps, theme: UiTheme, selected_index: usize) -> Vec<AnyElement<'static>> {
    let options = &props.snapshot.options;
    let len = options.len();
    let viewport_cap = list_viewport_cap(props.screen_height).max(1);
    let scroll_cap = viewport_cap.min(len.max(1));
    let window_start = palette_window_start(selected_index, scroll_cap, len);

    if props.snapshot.is_args_phase() {
        return options
            .iter()
            .enumerate()
            .skip(window_start)
            .take(scroll_cap)
            .map(|(i, opt)| palette_arg_row(&props.chrome, theme, &opt.name, &opt.description, i == selected_index))
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
            palette_command_row(
                &props.chrome,
                theme,
                &opt.name,
                cmd.args_hint.as_deref(),
                &opt.description,
                i == selected_index,
            )
        })
        .collect()
}

#[component]
pub fn PaletteCardBody(props: &PaletteCardBodyProps) -> impl Into<AnyElement<'static>> {
    let theme = props.theme.unwrap_or_default();
    let fixed_height = props.snapshot.list_height;

    if props.snapshot.has_matches() {
        let rows = palette_list_rows(props, theme, props.selected_index.map(|s| s.get()).unwrap_or(0));
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
        // Each item occupies exactly one row — body_height matches the item count.
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
                    color: theme.text_hint,
                    wrap: TextWrap::NoWrap,
                )
            }
        }
    }
}
