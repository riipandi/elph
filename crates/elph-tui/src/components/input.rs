//! Single-line text input (OpenTUI Input analogue).

use crate::text_editing::wire_input_shortcuts;
use iocraft::prelude::*;

use super::theme::{UiTheme, resolve_ui_theme};

/// Props for [`Input`].
#[derive(Default, Props)]
pub struct InputProps {
    pub width: u16,
    pub initial_value: String,
    pub has_focus: bool,
    pub text_color: Option<Color>,
    pub cursor_color: Option<Color>,
    pub focused_border_color: Option<Color>,
    pub value: Option<State<String>>,
    pub theme: Option<UiTheme>,
    /// Overrides [`UiTheme::input_inset`] (use `0` for flush dialog rows).
    pub inset: Option<u16>,
    /// Characters the parent handles as shortcuts — never inserted into the field.
    pub blocked_chars: Vec<char>,
    pub on_change: HandlerMut<'static, String>,
}

fn clamp_cursor_offset(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Single-line text input with optional external state.
#[component]
pub fn Input(props: &mut InputProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let external = props.value;
    let mut internal = hooks.use_state(|| {
        external
            .map(|state| state.read().clone())
            .unwrap_or_else(|| props.initial_value.clone())
    });

    let mut input_handle = hooks.use_ref_default::<TextInputHandle>();

    if let Some(parent) = external {
        let parent_text = parent.read().clone();
        if parent_text != internal.read().clone() {
            let cursor = clamp_cursor_offset(&parent_text, input_handle.read().cursor_offset());
            input_handle.write().set_cursor_offset(cursor);
            internal.set(parent_text);
        }
    }

    let mut value = internal;
    let has_focus = props.has_focus;
    let theme = resolve_ui_theme(&hooks, props.theme);
    let inset = props.inset.unwrap_or_else(|| theme.input_inset());
    let mut on_change = props.on_change.take();

    wire_input_shortcuts(&mut hooks, has_focus, value, input_handle);

    let display = value.read().clone();
    element! {
        View(
            width: props.width,
            background_color: Color::Reset,
            padding_left: inset,
            padding_right: inset,
        ) {
            TextInput(
                handle: Some(input_handle),
                has_focus: has_focus,
                multiline: false,
                blocked_chars: props.blocked_chars.clone(),
                color: props.text_color.unwrap_or_else(|| theme.input_text_color(has_focus)),
                cursor_color: props.cursor_color.unwrap_or_else(|| theme.input_cursor_color()),
                value: display,
                on_change: move |new_value: String| {
                    let prev = value.read().clone();
                    value.set(new_value.clone());
                    if let Some(mut parent) = external {
                        parent.set(new_value.clone());
                    }
                    if new_value != prev && !on_change.is_default() {
                        on_change(new_value);
                    }
                },
            )
        }
    }
}
