//! Ask-user option rows: label on top, dimmed hint or inline answer field underneath.

use elph_tui::components::DialogUserInputContent;
use elph_tui::components::theme::{UiTheme, dialog_option_desc_style, dialog_option_name_style, dialog_row_surface};
use elph_tui::list_selection_row_prefix;
use elph_tui::types::SelectOption;
use elph_tui::utils::wrap_text;
use iocraft::prelude::*;

/// Selection marker width (`❯ ` or `  `).
pub const ROW_PREFIX_CHARS: usize = 2;

/// Continuation-line indent matching [`ROW_PREFIX_CHARS`].
const ROW_CONTINUATION_INDENT: &str = "  ";

fn content_inner_width(list_width: u16) -> usize {
    list_width.saturating_sub(ROW_PREFIX_CHARS as u16).max(1) as usize
}

fn wrap_label_lines(name: &str, list_width: u16) -> Vec<String> {
    let mut lines = wrap_text(name, content_inner_width(list_width));
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn wrap_hint_lines(hint: &str, list_width: u16) -> Vec<String> {
    let mut lines = wrap_text(hint, content_inner_width(list_width));
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn format_label_lines(prefix: &str, name: &str, list_width: u16) -> String {
    let lines = wrap_label_lines(name, list_width);
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                format!("{prefix}{line}")
            } else {
                format!("{ROW_CONTINUATION_INDENT}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_hint_lines(hint: &str, list_width: u16) -> String {
    wrap_hint_lines(hint, list_width).join("\n")
}

fn option_row_height(name: &str, hint: &str, list_width: u16, inline_custom_input: bool) -> u16 {
    let label_lines = wrap_label_lines(name, list_width).len().max(1);
    let detail_lines = if inline_custom_input {
        1
    } else if hint.trim().is_empty() {
        0
    } else {
        wrap_hint_lines(hint, list_width).len()
    };
    (label_lines + detail_lines).max(1) as u16
}

/// Total terminal rows for all options (no inter-item gap).
#[cfg(test)]
pub fn option_list_total_rows(options: &[SelectOption], list_width: u16) -> u16 {
    option_list_total_rows_with_custom(options, list_width, None, false)
}

/// Like [`option_list_total_rows`] but accounts for an inline custom-answer field.
pub fn option_list_total_rows_with_custom(
    options: &[SelectOption],
    list_width: u16,
    custom_row_index: Option<usize>,
    custom_input_active: bool,
) -> u16 {
    if options.is_empty() {
        return 1;
    }
    options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let inline = custom_input_active && custom_row_index == Some(i);
            option_row_height(&opt.name, &opt.description, list_width, inline)
        })
        .sum::<u16>()
        .max(1)
}

#[derive(Default, Props)]
pub struct UserQuestionOptionListProps {
    pub width: u16,
    pub height: u16,
    pub options: Vec<SelectOption>,
    pub selected_index: Option<State<usize>>,
    pub has_focus: bool,
    pub theme: Option<UiTheme>,
    /// Index of the synthetic custom-answer row, when present.
    pub custom_row_index: Option<usize>,
    /// Replace the custom row hint with a flush inline text field.
    pub custom_input_active: bool,
    pub custom_answer: Option<State<String>>,
    pub custom_input_placeholder: String,
    pub custom_input_focused: bool,
    pub on_custom_submit: HandlerMut<'static, ()>,
    pub on_custom_cancel: HandlerMut<'static, ()>,
}

#[component]
pub fn UserQuestionOptionList(
    props: &mut UserQuestionOptionListProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = props.theme.unwrap_or_default();
    let internal_index = hooks.use_state(|| 0usize);
    let mut selected = props.selected_index.unwrap_or(internal_index);
    let has_focus = props.has_focus;
    let options = props.options.clone();
    let custom_row_index = props.custom_row_index;
    let custom_input_active = props.custom_input_active;
    let custom_input_placeholder = props.custom_input_placeholder.clone();
    let custom_input_focused = props.custom_input_focused;
    let hint_field_width = props.width.saturating_sub(ROW_PREFIX_CHARS as u16).max(1);

    let option_count = options.len();
    hooks.use_terminal_events(move |event| {
        if !has_focus || option_count == 0 {
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
        if modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT) {
            return;
        }
        let len = option_count;
        let prev = selected.get();
        let next = match code {
            KeyCode::Up | KeyCode::Char('k') => prev.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => (prev + 1).min(len - 1),
            _ => prev,
        };
        if next != prev {
            selected.set(next);
        }
    });

    let index = if option_count == 0 {
        0
    } else {
        selected.get().min(option_count - 1)
    };

    let rows: Vec<AnyElement<'static>> = if options.is_empty() {
        vec![
            element! {
                Text(content: "(no options)".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
            }
            .into(),
        ]
    } else {
        options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let selected_row = i == index;
                let prefix = list_selection_row_prefix(selected_row);
                let (name_color, name_weight) = dialog_option_name_style(theme, selected_row);
                let hint_color = dialog_option_desc_style(theme, selected_row);
                let inline_input = custom_input_active && custom_row_index == Some(i);
                let row_height = option_row_height(&opt.name, &opt.description, props.width, inline_input);
                let label_text = format_label_lines(prefix, &opt.name, props.width);
                let show_hint = !inline_input && !opt.description.trim().is_empty();
                let hint_text = format_hint_lines(&opt.description, props.width);
                let row_surface = dialog_row_surface(theme, selected_row);
                element! {
                    View(
                        width: props.width,
                        height: row_height,
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::FlexStart,
                        justify_content: JustifyContent::FlexStart,
                        gap: 0,
                        background_color: row_surface,
                        flex_shrink: 0f32,
                    ) {
                        View(width: props.width, flex_shrink: 0f32) {
                            Text(
                                content: label_text,
                                color: name_color,
                                weight: name_weight,
                                wrap: TextWrap::NoWrap,
                            )
                        }
                        #(if inline_input {
                            Some(element! {
                                View(
                                    width: props.width,
                                    padding_left: ROW_PREFIX_CHARS as u16,
                                    flex_shrink: 0f32,
                                ) {
                                    DialogUserInputContent(
                                        width: hint_field_width,
                                        question: String::new(),
                                        placeholder: custom_input_placeholder.clone(),
                                        value: props.custom_answer,
                                        has_focus: custom_input_focused,
                                        theme: Some(theme),
                                        section_gap: Some(0),
                                        show_prompt: false,
                                        show_footer_hint: false,
                                        show_placeholder_when_focused: false,
                                        dialog_chrome: true,
                                        compact: true,
                                        on_submit: props.on_custom_submit.take(),
                                        on_cancel: props.on_custom_cancel.take(),
                                    )
                                }
                            })
                        } else if show_hint {
                            Some(element! {
                                View(
                                    width: props.width,
                                    padding_left: ROW_PREFIX_CHARS as u16,
                                    flex_shrink: 0f32,
                                ) {
                                    Text(
                                        content: hint_text,
                                        color: hint_color,
                                        wrap: TextWrap::NoWrap,
                                    )
                                }
                            })
                        } else {
                            None
                        })
                    }
                }
                .into()
            })
            .collect()
    };

    let total_rows = option_list_total_rows_with_custom(&options, props.width, custom_row_index, custom_input_active);
    let viewport = if props.height == 0 || custom_input_active {
        total_rows.max(1)
    } else {
        props.height.min(total_rows).max(1)
    };

    if total_rows > viewport {
        element! {
            View(
                width: props.width,
                height: viewport,
                flex_direction: FlexDirection::Column,
                gap: 0,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
                #(rows)
            }
        }
    } else {
        element! {
            View(
                width: props.width,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(rows)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_tui::types::SelectOption;

    #[test]
    fn long_labels_wrap_instead_of_truncating() {
        let options = vec![
            SelectOption::new("A", "1"),
            SelectOption::new("A very long option label that should wrap", "2"),
        ];
        let list_width = 40u16;
        let rows = option_list_total_rows(&options, list_width);
        assert!(rows >= 2, "wrapped label should add at least one row");
        assert!(wrap_label_lines("A very long option label that should wrap", list_width).len() > 1);
    }

    #[test]
    fn stacked_row_counts_label_only_when_no_hint() {
        let options = vec![SelectOption::new("Yes", "")];
        let list_width = 30u16;
        assert_eq!(option_list_total_rows(&options, list_width), 1);
    }

    #[test]
    fn stacked_row_counts_label_and_hint_lines() {
        let options = vec![SelectOption::new("Yes", "confirm")];
        let list_width = 30u16;
        let rows = option_list_total_rows(&options, list_width);
        assert!(rows >= 2, "hint on its own line adds height below the label");
    }

    #[test]
    fn inline_custom_input_reserves_one_detail_row() {
        let options = vec![SelectOption::new("Red", ""), SelectOption::new("Other…", "")];
        let list_width = 40u16;
        let idle = option_list_total_rows_with_custom(&options, list_width, Some(1), false);
        let active = option_list_total_rows_with_custom(&options, list_width, Some(1), true);
        assert_eq!(idle, 2);
        assert_eq!(active, 3, "inline input adds one row under the custom label");
    }

    #[test]
    fn hint_uses_full_inner_width_below_label_indent() {
        let hint = "blue";
        let list_width = 24u16;
        let lines = wrap_hint_lines(hint, list_width);
        assert!(!lines.is_empty());
        assert!(content_inner_width(list_width) >= hint.chars().count());
    }
}
