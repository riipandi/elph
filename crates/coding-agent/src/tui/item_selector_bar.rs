//! Inline bar UI for [`super::item_selector::PendingItemSelector`].

use elph_tui::components::{SELECT_LIST_AUTO_HEIGHT, SelectList, UiTheme};
use iocraft::prelude::*;

use crate::tui::inline_dialog::{InlineDialogShell, OPTIONS_LIST_TOP_GAP, inline_body_width};
use crate::tui::item_selector::PendingItemSelector;

#[derive(Props)]
pub struct ItemSelectorBarProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub has_focus: bool,
    pub pending: Option<PendingItemSelector>,
    /// Absolute selected index state shared with the shell (SelectList expects State).
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
    let Some(pending) = props.pending.clone() else {
        return element! { View() }.into_any();
    };

    let options = pending.filtered_options();
    let filter_line = if pending.filter.is_empty() {
        "Filter: (type to search)".to_string()
    } else {
        format!("Filter: {}", pending.filter)
    };
    let empty = options.is_empty();
    let count_label = format!(
        "{} / {} item{}",
        options.len(),
        pending.items.len(),
        if pending.items.len() == 1 { "" } else { "s" }
    );

    // Cap list height so the dialog stays in the status zone budget.
    let max_list = ((props.screen_height / 3).clamp(6, 16)) as u16;
    let list_height = if empty {
        2
    } else if options.len() as u16 > max_list {
        max_list
    } else {
        SELECT_LIST_AUTO_HEIGHT
    };

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: pending.title.clone(),
            has_focus: props.has_focus,
            footer_hint: Some(pending.footer_hint.clone()),
        ) {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                View(width: body_width, flex_shrink: 0f32) {
                    Text(
                        content: filter_line,
                        color: theme.text_secondary,
                        wrap: TextWrap::Wrap,
                    )
                }
                View(width: body_width, flex_shrink: 0f32, padding_bottom: 0) {
                    Text(
                        content: count_label,
                        color: theme.text_muted,
                        wrap: TextWrap::Wrap,
                    )
                }
                View(
                    width: body_width,
                    padding_top: OPTIONS_LIST_TOP_GAP,
                    flex_shrink: 0f32,
                ) {
                    #(if empty {
                        element! {
                            Text(
                                content: "No matches".to_string(),
                                color: theme.text_secondary,
                                wrap: TextWrap::Wrap,
                            )
                        }.into_any()
                    } else {
                        element! {
                            SelectList(
                                width: body_width,
                                height: list_height,
                                options: options,
                                selected_index: props.selected_index,
                                has_focus: props.has_focus,
                                show_description: true,
                                compact: true,
                                theme: Some(theme),
                            )
                        }.into_any()
                    })
                }
            }
        }
    }
    .into_any()
}
