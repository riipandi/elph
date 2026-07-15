//! Multiline prompt editor (1-row default, grows with content).

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

fn insert_newline_at_cursor(text: &mut String, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    text.insert(cursor, '\n');
    cursor + 1
}

fn visible_row_count(text: &str, min_height: u16) -> u16 {
    let lines = text.lines().count().max(1) as u16;
    lines.max(min_height.max(1))
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
    let input_handle = hooks.use_ref_default::<TextInputHandle>();

    hooks.use_terminal_events({
        let mut input_handle = input_handle;
        move |event| {
            if !has_focus {
                return;
            }
            let TerminalEvent::Key(KeyEvent {
                code: KeyCode::Enter,
                kind,
                modifiers,
                ..
            }) = event
            else {
                return;
            };
            if kind == KeyEventKind::Release {
                return;
            }
            // Plain Enter is handled by the shell (submit). TextInput also treats Enter as
            // newline when multiline; the parent clears that via `suppress_enter_newline`.
            // Terminals often omit Shift on Enter, so provide Ctrl/Alt+Enter for newlines.
            if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                let cursor = input_handle.read().cursor_offset();
                let mut text = value.read().clone();
                let next = insert_newline_at_cursor(&mut text, cursor);
                value.set(text);
                input_handle.write().set_cursor_offset(next);
            }
        }
    });

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
                    if let Some(mut suppress) = suppress_enter_newline
                        && suppress.get()
                    {
                        suppress.set(false);
                        value.set(String::new());
                        return;
                    }
                    value.set(new_value);
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_newline_at_cursor_appends() {
        let mut text = "hi".to_string();
        let next = insert_newline_at_cursor(&mut text, 2);
        assert_eq!(text, "hi\n");
        assert_eq!(next, 3);
    }

    #[test]
    fn visible_row_count_grows_with_newlines() {
        assert_eq!(visible_row_count("one", 1), 1);
        assert_eq!(visible_row_count("a\nb", 1), 2);
        assert_eq!(visible_row_count("a", 3), 3);
    }
}
