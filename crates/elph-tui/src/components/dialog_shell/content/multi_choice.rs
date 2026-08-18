//! Multi-select question dialog body.

use super::layout::dialog_body_section_gap;
use crate::components::select::{
    SELECT_LIST_AUTO_HEIGHT, select_hidden_rows_above, select_inner_width, select_measured_row_counts,
    select_window_start_for_rows,
};
use crate::components::theme::{
    UiTheme, dialog_option_desc_style, dialog_option_name_style, dialog_row_surface, list_marker, resolve_ui_theme,
};
use crate::input_prefix::list_selection_row_prefix;
use crate::types::SelectOption;
use crate::utils::wrap_text;
use iocraft::prelude::*;

/// `❯ [ ] ` / `  [ ] ` prefix before the option name.
const INLINE_ROW_PREFIX_CHARS: usize = 6;

const INLINE_LABEL_CONTINUATION: &str = "      ";

fn multi_choice_row_surface(theme: UiTheme, focused: bool) -> Color {
    dialog_row_surface(theme, focused)
}

/// Toggle the checked state at `index` in `checked`.
pub fn multi_choice_toggle(checked: &mut [bool], index: usize) {
    if let Some(slot) = checked.get_mut(index) {
        *slot = !*slot;
    }
}

/// Selected option indices from parallel `checked` flags.
pub fn multi_choice_selected_indices(checked: &[bool]) -> Vec<usize> {
    checked
        .iter()
        .enumerate()
        .filter_map(|(i, on)| on.then_some(i))
        .collect()
}

/// Map a key press to multi-choice navigation or toggle (`None` = no change).
pub fn multi_choice_key_action(
    cursor: usize,
    len: usize,
    code: KeyCode,
    modifiers: KeyModifiers,
    step: usize,
) -> Option<MultiChoiceAction> {
    if len == 0 {
        return None;
    }
    if modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    let fast = modifiers.contains(KeyModifiers::SHIFT);
    match code {
        KeyCode::Up => {
            let delta = if fast { step } else { 1 };
            Some(MultiChoiceAction::MoveCursor(cursor.saturating_sub(delta)))
        }
        KeyCode::Down => {
            let delta = if fast { step } else { 1 };
            Some(MultiChoiceAction::MoveCursor((cursor + delta).min(len - 1)))
        }
        KeyCode::Char(' ') if modifiers.is_empty() => Some(MultiChoiceAction::ToggleCurrent),
        KeyCode::Enter if modifiers.is_empty() => Some(MultiChoiceAction::Submit),
        _ => None,
    }
}

/// Keyboard outcome for [`multi_choice_key_action`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiChoiceAction {
    MoveCursor(usize),
    ToggleCurrent,
    Submit,
}

/// Props for [`DialogMultiChoiceContent`].
#[derive(Props)]
pub struct DialogMultiChoiceContentProps {
    pub width: u16,
    pub height: u16,
    pub question: String,
    pub options: Vec<SelectOption>,
    pub cursor_index: Option<State<usize>>,
    pub checked: Option<State<Vec<bool>>>,
    pub has_focus: bool,
    pub show_description: bool,
    pub fast_scroll_step: usize,
    pub theme: Option<UiTheme>,
    /// Compact ask-user rows: label on top, dimmed hint below (vs. padded list style).
    pub inline_description: bool,
    /// When false, hides the Space/Enter hint line below the list.
    pub show_footer_hint: bool,
    pub on_submit: HandlerMut<'static, Vec<usize>>,
}

impl Default for DialogMultiChoiceContentProps {
    fn default() -> Self {
        Self {
            width: 40,
            height: SELECT_LIST_AUTO_HEIGHT,
            question: String::new(),
            options: Vec::new(),
            cursor_index: None,
            checked: None,
            has_focus: true,
            show_description: true,
            fast_scroll_step: 3,
            theme: None,
            inline_description: false,
            show_footer_hint: true,
            on_submit: HandlerMut::default(),
        }
    }
}

fn inline_content_inner_width(list_width: u16) -> usize {
    list_width.saturating_sub(INLINE_ROW_PREFIX_CHARS as u16).max(1) as usize
}

fn inline_wrap_label_lines(name: &str, list_width: u16) -> Vec<String> {
    let mut lines = wrap_text(name, inline_content_inner_width(list_width));
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn inline_wrap_hint_lines(hint: &str, list_width: u16) -> Vec<String> {
    let mut lines = wrap_text(hint, inline_content_inner_width(list_width));
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn inline_format_label_lines(prefix: &str, box_glyph: &str, name: &str, list_width: u16) -> String {
    let lines = inline_wrap_label_lines(name, list_width);
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                format!("{prefix}{box_glyph} {line}")
            } else {
                format!("{INLINE_LABEL_CONTINUATION}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn inline_format_hint_lines(hint: &str, list_width: u16) -> String {
    inline_wrap_hint_lines(hint, list_width).join("\n")
}

fn inline_row_height(name: &str, hint: &str, list_width: u16, show_description: bool) -> usize {
    let label_lines = inline_wrap_label_lines(name, list_width).len().max(1);
    let hint_lines = if show_description && !hint.trim().is_empty() {
        inline_wrap_hint_lines(hint, list_width).len()
    } else {
        0
    };
    (label_lines + hint_lines).max(1)
}

fn inline_row_counts(options: &[SelectOption], list_width: u16, show_description: bool) -> Vec<usize> {
    options
        .iter()
        .map(|opt| inline_row_height(&opt.name, &opt.description, list_width, show_description))
        .collect()
}

#[component]
pub fn DialogMultiChoiceContent(
    props: &mut DialogMultiChoiceContentProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let option_count = props.options.len();
    let internal_cursor = hooks.use_state(|| 0usize);
    let internal_checked = hooks.use_state(|| vec![false; option_count]);
    let cursor = props.cursor_index.unwrap_or(internal_cursor);
    let mut checked = props.checked.unwrap_or(internal_checked);
    let has_focus = props.has_focus;
    let step = props.fast_scroll_step.max(1);
    let show_description = props.show_description;
    let inline_description = props.inline_description;

    let checked_len = checked.read().len();
    if checked_len != option_count && option_count > 0 {
        checked.set(vec![false; option_count]);
    }

    hooks.use_terminal_events({
        let mut cursor = cursor;
        let mut checked = checked;
        let mut on_submit = props.on_submit.take();
        move |event| {
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
            match multi_choice_key_action(cursor.get(), option_count, code, modifiers, step) {
                Some(MultiChoiceAction::MoveCursor(next)) => cursor.set(next),
                Some(MultiChoiceAction::ToggleCurrent) => {
                    let idx = cursor.get().min(option_count.saturating_sub(1));
                    let mut flags = checked.read().clone();
                    multi_choice_toggle(&mut flags, idx);
                    checked.set(flags);
                }
                Some(MultiChoiceAction::Submit) => {
                    on_submit(multi_choice_selected_indices(&checked.read()));
                }
                None => {}
            }
        }
    });

    let index = cursor.get().min(option_count.saturating_sub(1));
    let flags = checked.read();
    let inner_width = if inline_description {
        props.width
    } else {
        select_inner_width(theme, props.width)
    };
    let container_surface = Color::Reset;
    let row_counts = if inline_description {
        inline_row_counts(&props.options, props.width, show_description)
    } else {
        select_measured_row_counts(&props.options, show_description, props.width, theme, false)
    };
    let total_rows: usize = row_counts.iter().sum::<usize>().max(1);
    let viewport_cap = if props.height == SELECT_LIST_AUTO_HEIGHT {
        total_rows
    } else {
        props.height.max(1) as usize
    };
    let viewport_rows = total_rows.max(1).min(viewport_cap);
    let scrollable = total_rows > viewport_rows;
    let window_start = select_window_start_for_rows(index, viewport_rows, &row_counts);

    let mut option_rows: Vec<AnyElement<'static>> = Vec::new();
    let mut used_rows = 0usize;

    if option_count == 0 {
        option_rows.push(
            element! {
                Text(content: "(no options)".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
            }
            .into(),
        );
    } else {
        let hidden_above = select_hidden_rows_above(window_start, &row_counts);
        if hidden_above > 0 {
            option_rows.push(
                element! {
                    Text(
                        content: format!("  ↑ {hidden_above} more"),
                        color: theme.text_hint,
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            );
            used_rows += 1;
        }

        for (i, opt) in props.options.iter().enumerate().skip(window_start) {
            let item_rows = row_counts[i];
            if used_rows > 0 && used_rows + item_rows > viewport_rows {
                break;
            }

            let row_focused = i == index;
            let is_on = flags.get(i).copied().unwrap_or(false);
            let box_glyph = if is_on { "[x]" } else { "[ ]" };
            let (name_color, name_weight) = dialog_option_name_style(theme, row_focused);
            let desc_color = dialog_option_desc_style(theme, row_focused);
            let show_desc = show_description && !opt.description.is_empty();

            if inline_description {
                let row_height = inline_row_counts(&props.options, props.width, show_description)[i] as u16;
                let label_text = inline_format_label_lines(
                    list_selection_row_prefix(row_focused),
                    box_glyph,
                    &opt.name,
                    props.width,
                );
                let show_hint = show_desc && !opt.description.trim().is_empty();
                let hint_text = inline_format_hint_lines(&opt.description, props.width);
                let row_surface = multi_choice_row_surface(theme, row_focused);
                option_rows.push(
                    element! {
                        View(
                            width: inner_width,
                            height: row_height,
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::FlexStart,
                            justify_content: JustifyContent::FlexStart,
                            gap: 0,
                            background_color: row_surface,
                            flex_shrink: 0f32,
                        ) {
                            View(width: inner_width, flex_shrink: 0f32) {
                                Text(
                                    content: label_text,
                                    color: name_color,
                                    weight: name_weight,
                                    wrap: TextWrap::NoWrap,
                                )
                            }
                            #(if show_hint {
                                Some(element! {
                                    View(
                                        width: inner_width,
                                        padding_left: INLINE_ROW_PREFIX_CHARS as u16,
                                        flex_shrink: 0f32,
                                    ) {
                                        Text(
                                            content: hint_text,
                                            color: desc_color,
                                            wrap: TextWrap::NoWrap,
                                        )
                                    }
                                })
                            } else {
                                None
                            })
                        }
                    }
                    .into(),
                );
            } else {
                let marker = list_marker(row_focused);
                option_rows.push(
                    element! {
                        View(
                            width: inner_width,
                            flex_direction: FlexDirection::Column,
                            gap: 0,
                            background_color: multi_choice_row_surface(theme, row_focused),
                            padding_left: theme.container_inset(),
                            padding_right: theme.container_inset(),
                            padding_top: if i > window_start { theme.gap_sm } else { 0 },
                        ) {
                            View(flex_direction: FlexDirection::Row, gap: theme.gap_md, align_items: AlignItems::Center) {
                                Text(content: format!("{marker}{box_glyph}"), color: name_color, wrap: TextWrap::NoWrap)
                                Text(content: opt.name.clone(), color: name_color, weight: name_weight, wrap: TextWrap::NoWrap)
                            }
                            #(if show_desc {
                                Some(element! {
                                    View(padding_left: theme.list_desc_padding_left()) {
                                        Text(
                                            content: opt.description.clone(),
                                            color: desc_color,
                                            wrap: TextWrap::Wrap,
                                        )
                                    }
                                })
                            } else {
                                None
                            })
                        }
                    }
                    .into(),
                );
            }
            used_rows += item_rows;
        }

        let rows_before = select_hidden_rows_above(window_start, &row_counts);
        let rows_shown = used_rows.saturating_sub(usize::from(hidden_above > 0));
        let hidden_below = row_counts
            .iter()
            .sum::<usize>()
            .saturating_sub(rows_before + rows_shown);
        if hidden_below > 0 {
            option_rows.push(
                element! {
                    Text(
                        content: format!("  ↓ {hidden_below} more"),
                        color: theme.text_hint,
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            );
        }
    }

    let section_gap = dialog_body_section_gap(theme);

    let list_viewport: AnyElement<'static> = if scrollable {
        element! {
            View(
                width: props.width,
                height: viewport_rows as u16,
                min_height: viewport_rows as u16,
                flex_direction: FlexDirection::Column,
                gap: if inline_description { 0 } else { theme.gap_sm },
                background_color: container_surface,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
                #(option_rows)
            }
        }
        .into()
    } else {
        element! {
            View(
                width: props.width,
                flex_direction: FlexDirection::Column,
                gap: if inline_description { 0 } else { theme.gap_sm },
                background_color: container_surface,
                flex_shrink: 0f32,
            ) {
                #(option_rows)
            }
        }
        .into()
    };

    let show_prompt = !props.question.is_empty();

    element! {
        View(
            width: props.width,
            flex_direction: FlexDirection::Column,
            gap: if inline_description && !show_prompt { 0 } else { section_gap },
            flex_shrink: 0f32,
        ) {
            #(if show_prompt {
                Some(element! {
                    Text(
                        content: props.question.clone(),
                        color: theme.text_secondary,
                        wrap: TextWrap::Wrap,
                    )
                })
            } else {
                None
            })
            #(list_viewport)
            #(if props.show_footer_hint {
                Some(element! {
                    Text(
                        content: "Space toggle · Enter confirm · ↑↓ move".to_string(),
                        color: theme.text_muted,
                        wrap: TextWrap::NoWrap,
                    )
                })
            } else {
                None
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_choice_toggle_flips_slot() {
        let mut flags = vec![false, true];
        multi_choice_toggle(&mut flags, 0);
        assert!(flags[0]);
        multi_choice_toggle(&mut flags, 1);
        assert!(!flags[1]);
    }

    #[test]
    fn multi_choice_selected_indices_collects_checked() {
        let flags = vec![true, false, true];
        assert_eq!(multi_choice_selected_indices(&flags), vec![0, 2]);
    }

    #[test]
    fn multi_choice_space_toggles() {
        assert_eq!(
            multi_choice_key_action(1, 3, KeyCode::Char(' '), KeyModifiers::empty(), 3),
            Some(MultiChoiceAction::ToggleCurrent)
        );
    }
}
