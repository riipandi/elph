//! Inline bar UI for [`super::item_selector::PendingItemSelector`].

use elph_tui::components::{SelectList, UiTheme};
use iocraft::prelude::*;

use crate::tui::inline_dialog::{InlineDialogShell, OPTIONS_LIST_TOP_GAP, inline_body_width};
use crate::tui::item_selector::PendingItemSelector;

/// Fixed list viewport rows — never auto-height for unbounded session trees.
const ITEM_SELECTOR_LIST_ROWS: u16 = 12;

#[derive(Props)]
pub struct ItemSelectorBarProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub has_focus: bool,
    pub pending: Option<PendingItemSelector>,
    /// SelectList highlight — update only on open / keys, never during render.
    pub selected_index: Option<State<usize>>,
    pub on_cancel: HandlerMut<'static, ()>,
}

impl Default for ItemSelectorBarProps {
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

#[component]
pub fn ItemSelectorBar(props: &mut ItemSelectorBarProps, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);

    if props.pending.is_none() {
        return element! { View() }.into_any();
    }
    let pending = props.pending.as_ref().expect("checked");

    let options = pending.filtered_options();
    let status = pending.status_line();
    let title = pending.title_with_mode();
    let empty = options.is_empty();
    let list_height = ITEM_SELECTOR_LIST_ROWS.min((props.screen_height / 3).clamp(6, 16));
    let footer = pending.footer_hint.clone();
    let selected = props.selected_index;
    let has_focus = props.has_focus;

    let list_body: AnyElement<'static> = if empty {
        element! {
            Text(
                content: "No matches for this filter".to_string(),
                color: theme.text_secondary,
                wrap: TextWrap::Wrap,
            )
        }
        .into_any()
    } else {
        element! {
            SelectList(
                width: body_width,
                height: list_height,
                options: options,
                selected_index: selected,
                has_focus: has_focus,
                show_description: true,
                compact: true,
                theme: Some(theme),
            )
        }
        .into_any()
    };

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: title,
            has_focus: has_focus,
            footer_hint: Some(footer),
        ) {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                View(width: body_width, flex_shrink: 0f32) {
                    Text(
                        content: status,
                        color: theme.text_secondary,
                        wrap: TextWrap::Wrap,
                    )
                }
                View(
                    width: body_width,
                    padding_top: OPTIONS_LIST_TOP_GAP,
                    height: list_height,
                    overflow: Overflow::Hidden,
                    flex_shrink: 0f32,
                ) {
                    #(list_body)
                }
            }
        }
    }
    .into_any()
}
