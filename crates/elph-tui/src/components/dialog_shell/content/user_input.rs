//! Free-text user answer dialog body.

use super::layout::dialog_body_section_gap;
use crate::components::Input;
use crate::components::theme::{UiTheme, resolve_ui_theme};
use iocraft::prelude::*;

/// Props for [`DialogUserInputContent`].
#[derive(Props)]
pub struct DialogUserInputContentProps {
    pub width: u16,
    pub question: String,
    pub placeholder: String,
    pub value: Option<State<String>>,
    pub has_focus: bool,
    pub theme: Option<UiTheme>,
    pub section_gap: Option<u16>,
    /// When false, the prompt is shown only in the dialog header (compact inline layout).
    pub show_prompt: bool,
    /// When false, the inline key hint line below the field is hidden.
    pub show_footer_hint: bool,
    /// When true and the value is empty, show dim placeholder text beside the caret (not over it).
    pub show_placeholder_when_focused: bool,
    /// Warm dialog palette for ask-user inline fields (no blue caret/underline).
    pub dialog_chrome: bool,
    /// Flush single-line field for inline dialog rows (no underline bar).
    pub compact: bool,
    /// Characters the parent handles as shortcuts — never inserted into the field.
    pub blocked_chars: Vec<char>,
    pub on_submit: HandlerMut<'static, ()>,
    pub on_cancel: HandlerMut<'static, ()>,
}

impl Default for DialogUserInputContentProps {
    fn default() -> Self {
        Self {
            width: 40,
            question: String::new(),
            placeholder: String::new(),
            value: None,
            has_focus: true,
            theme: None,
            section_gap: None,
            show_prompt: true,
            show_footer_hint: true,
            show_placeholder_when_focused: false,
            dialog_chrome: false,
            compact: false,
            blocked_chars: Vec::new(),
            on_submit: HandlerMut::default(),
            on_cancel: HandlerMut::default(),
        }
    }
}

/// Ask-user body with a prompt line and single-line text field.
#[component]
pub fn DialogUserInputContent(
    props: &mut DialogUserInputContentProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let internal = hooks.use_state(String::new);
    let value = props.value.unwrap_or(internal);
    let placeholder = if props.placeholder.is_empty() {
        "Type your answer…".to_string()
    } else {
        props.placeholder.clone()
    };

    let section_gap = props.section_gap.unwrap_or_else(|| dialog_body_section_gap(theme));
    let has_focus = props.has_focus;
    let value_empty = value.read().is_empty();
    let show_placeholder = value_empty && !has_focus;
    let show_focused_placeholder = value_empty && has_focus && props.show_placeholder_when_focused;
    let placeholder_color = if has_focus { theme.text_hint } else { theme.text_muted };
    let text_color = if props.dialog_chrome {
        theme.dialog_input_text_color(has_focus)
    } else {
        theme.input_text_color(has_focus)
    };
    let cursor_color = if props.dialog_chrome {
        theme.dialog_input_cursor_color()
    } else {
        theme.input_cursor_color()
    };
    let underline_color = if props.dialog_chrome {
        theme.dialog_input_underline_color(has_focus)
    } else if has_focus {
        theme.border_focus
    } else {
        theme.border_subtle
    };

    hooks.use_terminal_events({
        let mut on_submit = props.on_submit.take();
        let mut on_cancel = props.on_cancel.take();
        move |event| {
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
            if !modifiers.is_empty() {
                return;
            }
            match code {
                KeyCode::Enter if !on_submit.is_default() => on_submit(()),
                KeyCode::Esc if !on_cancel.is_default() => on_cancel(()),
                _ => {}
            }
        }
    });

    let placeholder_overlay: Option<AnyElement<'static>> = if show_placeholder {
        Some(
            element! {
                View(
                    position: Position::Absolute,
                    top: 0,
                    left: 0,
                    width: props.width,
                    background_color: Color::Reset,
                ) {
                    Text(
                        content: placeholder.clone(),
                        color: placeholder_color,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            .into(),
        )
    } else if show_focused_placeholder {
        Some(
            element! {
                View(
                    position: Position::Absolute,
                    top: 0,
                    left: 1,
                    width: props.width.saturating_sub(1).max(1),
                    background_color: Color::Reset,
                ) {
                    Text(
                        content: placeholder.clone(),
                        color: placeholder_color,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            .into(),
        )
    } else {
        None
    };

    element! {
        View(
            width: props.width,
            flex_direction: FlexDirection::Column,
            gap: section_gap,
            flex_shrink: 0f32,
        ) {
            #(if props.show_prompt {
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
            View(
                width: props.width,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                View(
                    width: props.width,
                    position: Position::Relative,
                    flex_shrink: 0f32,
                ) {
                    Input(
                        width: props.width,
                        value: Some(value),
                        has_focus: has_focus,
                        blocked_chars: props.blocked_chars.clone(),
                        text_color: Some(text_color),
                        cursor_color: Some(cursor_color),
                        inset: Some(if props.compact { 0 } else { theme.input_inset() }),
                        theme: Some(theme),
                    )
                    #(placeholder_overlay)
                }
                #(if !props.compact {
                    Some(element! {
                        View(
                            width: props.width,
                            height: 1,
                            background_color: underline_color,
                            flex_shrink: 0f32,
                        )
                    })
                } else {
                    None
                })
            }
            #(if props.show_footer_hint {
                Some(element! {
                    Text(
                        content: format!("{placeholder} · Enter submit · Esc cancel"),
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
