//! Single-line text input (OpenTUI Input analogue).

use crate::text_editing::wire_input_shortcuts;
use iocraft::prelude::*;

/// Props for [`Input`].
#[derive(Clone, Default, Props)]
pub struct InputProps {
    pub width: u16,
    pub initial_value: String,
    pub has_focus: bool,
    pub text_color: Option<Color>,
    pub cursor_color: Option<Color>,
    pub focused_border_color: Option<Color>,
    pub value: Option<State<String>>,
}

/// Single-line text input with optional external state.
#[component]
pub fn Input(props: &InputProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let internal = hooks.use_state(|| props.initial_value.clone());
    let mut value = props.value.unwrap_or(internal);
    let has_focus = props.has_focus;
    let input_handle = hooks.use_ref_default::<TextInputHandle>();

    wire_input_shortcuts(&mut hooks, has_focus, value, input_handle);

    element! {
        View(
            width: props.width,
            border_style: if has_focus { BorderStyle::Round } else { BorderStyle::None },
            border_color: props.focused_border_color.unwrap_or(Color::Blue),
        ) {
            TextInput(
                handle: Some(input_handle),
                has_focus: has_focus,
                multiline: false,
                color: props.text_color.unwrap_or(Color::Grey),
                cursor_color: props.cursor_color.unwrap_or(Color::DarkGrey),
                value: value.to_string(),
                on_change: move |new_value| value.set(new_value),
            )
        }
    }
}
