//! Inline bar UI for [`super::thinking_selector::PendingThinkingSelector`].
//!
//! Full-width list rows (same stretch as the slash command palette rows): each row
//! spans the content width, selected row shows the `❯` marker. Dense one-header
//! dialog — no divider, no status row.

use elph_tui::components::theme::{UiTheme, dialog_option_name_style, dialog_row_surface};
use elph_tui::list_selection_row_prefix;
use iocraft::prelude::*;

use crate::tui::inline_dialog::{InlineDialogShell, OPTIONS_LIST_TOP_GAP, inline_body_width};
use crate::tui::thinking_selector::PendingThinkingSelector;

const FOOTER_HINT: &str = "↑↓ move · Enter set · Esc cancel";

#[derive(Props)]
pub struct ThinkingSelectorBarProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub has_focus: bool,
    pub pending: Option<PendingThinkingSelector>,
    /// SelectList highlight — update only on open / keys, never during render.
    pub selected_index: Option<State<usize>>,
    pub on_cancel: HandlerMut<'static, ()>,
}

impl Default for ThinkingSelectorBarProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            has_focus: false,
            pending: None,
            selected_index: None,
            on_cancel: HandlerMut::default(),
        }
    }
}

fn thinking_row(theme: UiTheme, level: &str, selected: bool) -> AnyElement<'static> {
    let prefix = list_selection_row_prefix(selected);
    let (name_color, name_weight) = dialog_option_name_style(theme, selected);
    let row_surface = dialog_row_surface(theme, selected);
    element! {
        View(
            width: 100pct,
            height: 1u16,
            background_color: row_surface,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            justify_content: JustifyContent::FlexStart,
            flex_shrink: 0f32,
            overflow: Overflow::Hidden,
        ) {
            Text(
                content: format!("{prefix}{level}"),
                color: name_color,
                weight: name_weight,
                wrap: TextWrap::NoWrap,
                align: TextAlign::Left,
            )
        }
    }
    .into()
}

#[component]
pub fn ThinkingSelectorBar(props: &mut ThinkingSelectorBarProps, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);

    if props.pending.is_none() {
        return element! { View() }.into_any();
    }
    let pending = props.pending.as_ref().expect("checked");
    let selected = props.selected_index.map(|s| s.get()).unwrap_or(0);

    let rows: Vec<AnyElement<'static>> = pending
        .levels
        .iter()
        .enumerate()
        .map(|(i, level)| thinking_row(theme, level.label(), i == selected))
        .collect();

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: "Thinking Level".to_string(),
            has_focus: props.has_focus,
            footer_hint: Some(FOOTER_HINT.to_string()),
        ) {
            View(
                width: body_width,
                padding_top: OPTIONS_LIST_TOP_GAP,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(rows)
            }
        }
    }
    .into_any()
}
