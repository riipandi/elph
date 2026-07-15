//! Multiline prompt editor (1-row default, grows with content).

use crate::text_editing::wire_editing_shortcuts;
use iocraft::prelude::*;

/// Props for [`Textarea`].
#[derive(Clone, Default, Props)]
pub struct TextareaProps {
    pub width: u16,
    /// Minimum visible rows. Defaults to 1 when unset or zero.
    pub min_height: u16,
    pub initial_value: String,
    pub has_focus: bool,
    pub text_color: Option<Color>,
    pub cursor_color: Option<Color>,
    pub value: Option<State<String>>,
    /// When false, omits the inner border (for embedding in a parent chrome).
    pub show_border: Option<bool>,
    /// Set by the parent before submit so `TextInput` does not leave a stray `\n`.
    pub suppress_enter_newline: Option<Ref<bool>>,
}

fn visible_row_count(text: &str, min_height: u16) -> u16 {
    let lines = text.lines().count().max(1) as u16;
    lines.max(min_height.max(1))
}

/// While suppression is active, keep real keystrokes and drop only ghost trailing newlines.
fn resolve_suppressed_change(new_value: String) -> String {
    if new_value.ends_with('\n') {
        String::new()
    } else {
        new_value
    }
}

/// Apply a [`TextInput`] change while optionally swallowing the ghost `\n` multiline Enter adds on submit.
fn apply_text_input_change(
    suppress_enter_newline: Option<Ref<bool>>,
    value: &mut State<String>,
    input_handle: &mut Ref<TextInputHandle>,
    new_value: String,
) {
    if let Some(mut suppress) = suppress_enter_newline
        && suppress.get()
    {
        suppress.set(false);
        let resolved = resolve_suppressed_change(new_value);
        if resolved.is_empty() {
            input_handle.write().set_cursor_offset(0);
        }
        value.set(resolved);
        return;
    }
    value.set(new_value);
}

/// Multiline text input with optional external state.
#[component]
pub fn Textarea(props: &TextareaProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let internal = hooks.use_state(|| props.initial_value.clone());
    let mut value = props.value.unwrap_or(internal);
    let suppress_enter_newline = props.suppress_enter_newline;
    let has_focus = props.has_focus;
    let min_height = props.min_height.max(1);
    let show_border = props.show_border.unwrap_or(true);
    let mut input_handle = hooks.use_ref_default::<TextInputHandle>();

    // Plain Enter is handled by the shell (submit). Ctrl/Alt+Enter inserts a newline.
    wire_editing_shortcuts(&mut hooks, has_focus, true, value, input_handle);

    let height = visible_row_count(&value.read(), min_height);
    let border_style = if show_border && has_focus {
        BorderStyle::Round
    } else if show_border {
        BorderStyle::Single
    } else {
        BorderStyle::None
    };

    element! {
        View(
            width: props.width,
            min_height: height,
            border_style: border_style,
            border_color: Color::DarkGrey,
            padding_left: if show_border { 1 } else { 0 },
            padding_right: if show_border { 1 } else { 0 },
        ) {
            TextInput(
                handle: Some(input_handle),
                has_focus: has_focus,
                multiline: true,
                color: props.text_color.unwrap_or(Color::Grey),
                cursor_color: props.cursor_color.unwrap_or(Color::DarkGrey),
                value: value.to_string(),
                on_change: move |new_value| {
                    apply_text_input_change(
                        suppress_enter_newline,
                        &mut value,
                        &mut input_handle,
                        new_value,
                    );
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_editing::insert_newline_at_cursor;

    #[test]
    fn insert_newline_at_cursor_appends() {
        let (text, next) = insert_newline_at_cursor("hi", 2);
        assert_eq!(text, "hi\n");
        assert_eq!(next, 3);
    }

    #[test]
    fn visible_row_count_grows_with_newlines() {
        assert_eq!(visible_row_count("one", 1), 1);
        assert_eq!(visible_row_count("a\nb", 1), 2);
        assert_eq!(visible_row_count("a", 3), 3);
    }

    #[test]
    fn resolve_suppressed_change_keeps_first_typed_char() {
        assert_eq!(resolve_suppressed_change("a".into()), "a");
        assert_eq!(resolve_suppressed_change("ab".into()), "ab");
    }

    #[test]
    fn resolve_suppressed_change_drops_ghost_newlines() {
        assert_eq!(resolve_suppressed_change("\n".into()), "");
        assert_eq!(resolve_suppressed_change("hello\n".into()), "");
    }
}
