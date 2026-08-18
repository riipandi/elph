//! Horizontal tab selector with content panel.

use crate::types::TabItem;
use iocraft::prelude::*;

use super::theme::{UiTheme, resolve_ui_theme, tab_styles};

/// Tab chrome for one tab label.
pub fn tab_select_tab_styles(theme: UiTheme, active: bool) -> (BorderStyle, Color, Weight) {
    tab_styles(theme, active)
}

/// Active tab index clamped to list bounds.
pub fn tab_select_clamped_index(selected: usize, len: usize) -> usize {
    if len == 0 { 0 } else { selected.min(len - 1) }
}

/// Next tab index after a key press.
pub fn tab_select_key_to_index(current: usize, len: usize, code: KeyCode) -> usize {
    if len == 0 {
        return 0;
    }
    match code {
        KeyCode::Left => current.saturating_sub(1),
        KeyCode::Right | KeyCode::Tab => (current + 1).min(len - 1),
        KeyCode::BackTab => current.saturating_sub(1),
        _ => current,
    }
}

/// Props for [`TabSelect`].
#[derive(Default, Props)]
pub struct TabSelectProps {
    pub width: u16,
    pub tabs: Vec<TabItem>,
    pub selected_index: Option<State<usize>>,
    pub has_focus: bool,
    pub theme: Option<UiTheme>,
    pub on_change: HandlerMut<'static, usize>,
}

/// Tab row with a single active content panel below.
#[component]
pub fn TabSelect(props: &mut TabSelectProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let internal = hooks.use_state(|| 0usize);
    let mut selected = props.selected_index.unwrap_or(internal);
    let has_focus = props.has_focus;
    let tabs = props.tabs.clone();
    let len = tabs.len();
    let theme = resolve_ui_theme(&hooks, props.theme);

    hooks.use_terminal_events({
        let mut on_change = props.on_change.take();
        move |event| {
            if !has_focus || len == 0 {
                return;
            }
            let TerminalEvent::Key(KeyEvent { code, kind, .. }) = event else {
                return;
            };
            if kind == KeyEventKind::Release {
                return;
            }
            let prev = selected.get();
            let next = tab_select_key_to_index(prev, len, code);
            if next != prev {
                selected.set(next);
                if !on_change.is_default() {
                    on_change(next);
                }
            }
        }
    });

    let index = tab_select_clamped_index(selected.get(), len);
    let active_content = tabs.get(index).map(|t| t.content.clone()).unwrap_or_default();

    let tab_labels: Vec<_> = tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let active_tab = i == index;
            let (border_style, color, weight) = tab_styles(theme, active_tab);
            element! {
                View(
                    border_style: border_style,
                    border_color: color,
                    padding_left: theme.padding_sm,
                    padding_right: theme.padding_sm,
                ) {
                    Text(
                        content: tab.label.clone(),
                        color: color,
                        weight: weight,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
        })
        .collect();

    element! {
        View(width: props.width, flex_direction: FlexDirection::Column, gap: theme.gap_md) {
            View(flex_direction: FlexDirection::Row, gap: theme.gap_md, flex_wrap: FlexWrap::Wrap) {
                #(tab_labels)
            }
            View(
                width: props.width,
                border_style: theme.container_border(has_focus),
                border_color: theme.container_border_color(has_focus),
                padding: theme.container_inset(),
                min_height: theme.panel_min_height(),
                background_color: theme.list_surface(),
                overflow: Overflow::Hidden,
            ) {
                Text(content: active_content, color: theme.text_secondary, wrap: TextWrap::Wrap)
            }
            #(if has_focus {
                Some(element! {
                    Text(
                        content: "←/→ or Tab switch tabs".to_string(),
                        color: theme.text_hint,
                        wrap: TextWrap::NoWrap,
                    )
                })
            } else {
                None
            })
        }
    }
}
