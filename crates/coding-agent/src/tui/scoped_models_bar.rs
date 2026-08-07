//! Inline dialog UI for `/scoped-models`.

use elph_tui::components::DialogUserInputContent;
use elph_tui::components::theme::{UiTheme, dialog_option_desc_style, dialog_option_name_style, dialog_row_surface};
use elph_tui::list_selection_row_prefix;
use iocraft::prelude::*;

use crate::tui::inline_dialog::{InlineDialogShell, OPTIONS_LIST_TOP_GAP, inline_body_width};
use crate::tui::model_selector::model_selector_list_viewport_height;
use crate::tui::scoped_models::{PendingScopedModels, ScopedModelItem};
use crate::tui::slash_palette::palette_window_start;

/// Render snapshot for [`ScopedModelsBar`].
#[derive(Debug, Clone)]
pub struct ScopedModelsView {
    pub items: Vec<ScopedModelItem>,
    pub selected_index: usize,
    pub footer_hint: String,
    pub count_label: String,
    pub detail_line: Option<String>,
}

impl ScopedModelsView {
    pub fn from_pending(pending: &PendingScopedModels) -> Self {
        let items = pending.filtered_items();
        let detail_line = items
            .get(pending.selected_index)
            .map(|item| format!("Model Name: {}", item.name));
        Self {
            count_label: format!(
                "{} models · {}/{} enabled for Ctrl+P",
                pending.total_count(),
                pending.enabled_count(),
                pending.total_count()
            ),
            items,
            selected_index: pending.selected_index,
            footer_hint: pending.footer_hint(),
            detail_line,
        }
    }
}

#[derive(Props)]
pub struct ScopedModelsBarProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub view: ScopedModelsView,
    pub selected_index: Option<State<usize>>,
    pub filter: Option<State<String>>,
    pub has_focus: bool,
    pub on_filter_submit: HandlerMut<'static, ()>,
    pub on_cancel: HandlerMut<'static, ()>,
}

impl Default for ScopedModelsBarProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            view: ScopedModelsView {
                items: Vec::new(),
                selected_index: 0,
                footer_hint: String::new(),
                count_label: String::new(),
                detail_line: None,
            },
            selected_index: None,
            filter: None,
            has_focus: false,
            on_filter_submit: HandlerMut::default(),
            on_cancel: HandlerMut::default(),
        }
    }
}

fn status_mark(enabled: bool) -> &'static str {
    if enabled { "✓" } else { "✗" }
}

fn scoped_row(theme: UiTheme, list_width: u16, item: &ScopedModelItem, selected: bool) -> AnyElement<'static> {
    let prefix = list_selection_row_prefix(selected);
    let (name_color, name_weight) = dialog_option_name_style(theme, selected);
    let desc_color = dialog_option_desc_style(theme, selected);
    let status_color = if item.enabled {
        Color::Rgb { r: 129, g: 199, b: 132 }
    } else {
        theme.text_muted
    };
    let row_surface = dialog_row_surface(theme, selected);
    let label = format!("{prefix}{}", item.model_id);
    let hint = format!("[{}] {}", item.provider_id, item.name);
    let mark = status_mark(item.enabled);

    element! {
        View(
            width: list_width,
            height: 1,
            background_color: row_surface,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            gap: 1,
            flex_shrink: 0f32,
        ) {
            View(width: list_width.saturating_sub(4).max(8), height: 1, flex_direction: FlexDirection::Row, gap: 1) {
                Text(
                    content: label,
                    color: name_color,
                    weight: name_weight,
                    wrap: TextWrap::NoWrap,
                )
                Text(
                    content: hint,
                    color: desc_color,
                    wrap: TextWrap::NoWrap,
                )
            }
            Text(content: mark.to_string(), color: status_color, wrap: TextWrap::NoWrap)
        }
    }
    .into()
}

#[component]
pub fn ScopedModelsBar(props: &mut ScopedModelsBarProps, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);
    let list_height = model_selector_list_viewport_height(props.screen_width, props.screen_height);
    let option_count = props.view.items.len();
    let index = if option_count == 0 {
        0
    } else {
        props.view.selected_index.min(option_count - 1)
    };
    let scroll_cap = (list_height as usize).max(1).min(option_count.max(1));
    let window_start = palette_window_start(index, scroll_cap, option_count);

    let rows: Vec<AnyElement<'static>> = if props.view.items.is_empty() {
        vec![
            element! {
                Text(content: "(no matching models)".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
            }
            .into(),
        ]
    } else {
        props
            .view
            .items
            .iter()
            .enumerate()
            .skip(window_start)
            .take(scroll_cap)
            .map(|(i, item)| scoped_row(theme, body_width, item, i == index))
            .collect()
    };

    let detail = props.view.detail_line.clone().unwrap_or_default();

    let body = element! {
        View(
            width: body_width,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            Text(
                content: props.view.count_label.clone(),
                color: theme.text_muted,
                wrap: TextWrap::NoWrap,
            )
            View(width: body_width, padding_top: 1, flex_shrink: 0f32) {
                DialogUserInputContent(
                    width: body_width,
                    question: String::new(),
                    placeholder: "Filter models…".to_string(),
                    value: props.filter,
                    has_focus: props.has_focus,
                    theme: Some(theme),
                    show_prompt: false,
                    show_footer_hint: false,
                    show_placeholder_when_focused: true,
                    dialog_chrome: true,
                    compact: true,
                    on_submit: props.on_filter_submit.take(),
                    on_cancel: props.on_cancel.take(),
                )
            }
            View(
                width: body_width,
                padding_top: OPTIONS_LIST_TOP_GAP,
                height: list_height.max(1),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
                #(rows)
            }
            #(if detail.is_empty() {
                None
            } else {
                Some(element! {
                    View(width: body_width, padding_top: 1, flex_shrink: 0f32) {
                        Text(content: detail, color: theme.text_muted, wrap: TextWrap::NoWrap)
                    }
                })
            })
        }
    };

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: "Scoped models".to_string(),
            has_focus: props.has_focus,
            footer_hint: Some(props.view.footer_hint.clone()),
        ) {
            #(body)
        }
    }
}
