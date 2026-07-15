//! Vertical list selector (OpenTUI Select analogue).

use crate::types::SelectOption;
use iocraft::prelude::*;

/// Props for [`SelectList`].
#[derive(Clone, Default, Props)]
pub struct SelectListProps {
    pub width: u16,
    pub height: u16,
    pub options: Vec<SelectOption>,
    pub selected_index: Option<State<usize>>,
    pub has_focus: bool,
    pub show_description: bool,
    pub fast_scroll_step: usize,
}

/// Keyboard-navigable option list.
#[component]
pub fn SelectList(props: &SelectListProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let internal_index = hooks.use_state(|| 0usize);
    let mut selected = props.selected_index.unwrap_or(internal_index);
    let step = props.fast_scroll_step.max(1);
    let has_focus = props.has_focus;
    let options = props.options.clone();
    let show_description = props.show_description;
    let height = props.height.max(1) as usize;

    let option_count = options.len();
    hooks.use_terminal_events(move |event| {
        if !has_focus {
            return;
        }
        let TerminalEvent::Key(KeyEvent {
            code, kind, modifiers, ..
        }) = event
        else {
            return;
        };
        if kind == KeyEventKind::Release {
            return;
        }

        let len = option_count;
        if len == 0 {
            return;
        }

        let fast = modifiers.contains(KeyModifiers::SHIFT);
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                let delta = if fast { step } else { 1 };
                selected.set(selected.get().saturating_sub(delta));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let delta = if fast { step } else { 1 };
                selected.set((selected.get() + delta).min(len - 1));
            }
            KeyCode::Enter => {}
            _ => {}
        }
    });

    let len = options.len();
    let index = if len == 0 { 0 } else { selected.get().min(len - 1) };
    let window_start = index.saturating_sub(height / 2).min(len.saturating_sub(1));

    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    if len == 0 {
        rows.push(element! { Text(content: "(no options)", color: Color::DarkGrey) }.into());
    } else {
        for (i, opt) in options.iter().enumerate().skip(window_start).take(height) {
            let selected_row = i == index;
            let name_color = if selected_row { Color::Yellow } else { Color::Grey };
            let prefix = if selected_row { "› " } else { "  " };
            let desc = if show_description && !opt.description.is_empty() {
                format!("\n   {}", opt.description)
            } else {
                String::new()
            };
            rows.push(
                element! {
                    Text(
                        content: format!("{prefix}{}{desc}", opt.name),
                        color: name_color,
                        weight: if selected_row { Weight::Bold } else { Weight::Normal },
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            );
        }
    }

    element! {
        View(
            width: props.width,
            height: props.height,
            flex_direction: FlexDirection::Column,
            border_style: if has_focus { BorderStyle::Single } else { BorderStyle::None },
            border_color: Color::DarkGrey,
            padding_left: 1,
            padding_right: 1,
        ) {
            #(rows)
        }
    }
}
