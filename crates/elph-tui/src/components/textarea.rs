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
    /// Set by the parent on submit so plain Enter's ghost `\n` is dropped, not the next keystroke.
    pub suppress_enter_newline: Option<Ref<bool>>,
}

/// Logical row count, including an empty row after a trailing `\n`.
pub fn logical_line_count(text: &str) -> u16 {
    let lines = text.chars().filter(|&c| c == '\n').count() + 1;
    lines.max(1) as u16
}

fn newline_count(text: &str) -> usize {
    text.chars().filter(|&c| c == '\n').count()
}

/// Byte offset after a single `\n` inserted at `prev_cursor`.
fn cursor_after_newline_insertion(prev: &str, new: &str, prev_cursor: usize) -> usize {
    if newline_count(new) <= newline_count(prev) {
        return prev_cursor.min(new.len());
    }
    (prev_cursor + '\n'.len_utf8()).min(new.len())
}

/// While suppression is active, keep real keystrokes and drop only ghost trailing newlines.
fn resolve_suppressed_change(new_value: String) -> String {
    if new_value.ends_with('\n') {
        String::new()
    } else {
        new_value
    }
}

fn apply_text_input_change(
    suppress_enter_newline: Option<Ref<bool>>,
    value: &mut State<String>,
    input_handle: &mut Ref<TextInputHandle>,
    mut cursor_snapshot: Ref<usize>,
    new_value: String,
) {
    let prev = value.read().clone();
    let prev_cursor = input_handle.read().cursor_offset();

    if let Some(mut suppress) = suppress_enter_newline
        && suppress.get()
    {
        suppress.set(false);
        let resolved = resolve_suppressed_change(new_value);
        if resolved.is_empty() {
            cursor_snapshot.set(0);
            input_handle.write().set_cursor_offset(0);
        }
        value.set(resolved);
        return;
    }

    value.set(new_value.clone());
    if newline_count(&new_value) > newline_count(&prev) {
        let next_cursor = cursor_after_newline_insertion(&prev, &new_value, prev_cursor);
        cursor_snapshot.set(next_cursor);
        input_handle.write().set_cursor_offset(next_cursor);
    }
}

fn visible_row_count(text: &str, min_height: u16) -> u16 {
    logical_line_count(text).max(min_height.max(1))
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
    let cursor_snapshot = hooks.use_ref(|| 0usize);

    // Enter submits in the shell; Shift+Enter newlines via multiline TextInput; Ctrl+J via shortcuts.
    wire_editing_shortcuts(&mut hooks, has_focus, true, value, input_handle, cursor_snapshot);

    let row_count = visible_row_count(&value.read(), min_height);

    // Remount TextInput when row count changes so iocraft clears a stale vertical scroll offset.
    hooks.use_effect(
        {
            let mut input_handle = input_handle;
            let cursor_snapshot = cursor_snapshot;
            move || {
                input_handle.write().set_cursor_offset(cursor_snapshot.get());
            }
        },
        row_count,
    );
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
            height: row_count,
            min_height: row_count,
            flex_shrink: 0f32,
            border_style: border_style,
            border_color: Color::DarkGrey,
            padding_left: if show_border { 1 } else { 0 },
            padding_right: if show_border { 1 } else { 0 },
        ) {
            TextInput(
                key: row_count,
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
                        cursor_snapshot,
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
    fn resolve_suppressed_change_keeps_first_typed_char() {
        assert_eq!(resolve_suppressed_change("a".into()), "a");
    }

    #[test]
    fn resolve_suppressed_change_drops_ghost_newlines() {
        assert_eq!(resolve_suppressed_change("\n".into()), "");
    }

    #[test]
    fn logical_line_count_includes_trailing_newline_row() {
        assert_eq!(logical_line_count("hello"), 1);
        assert_eq!(logical_line_count("hello\n"), 2);
        assert_eq!(logical_line_count("a\nb\n"), 3);
    }

    #[test]
    fn visible_row_count_grows_with_newlines() {
        assert_eq!(visible_row_count("one", 1), 1);
        assert_eq!(visible_row_count("a\nb", 1), 2);
        assert_eq!(visible_row_count("hello\n", 1), 2);
        assert_eq!(visible_row_count("a", 3), 3);
    }

    #[test]
    fn cursor_after_newline_insertion_advances_one_byte() {
        assert_eq!(cursor_after_newline_insertion("hi", "hi\n", 2), 3);
        assert_eq!(cursor_after_newline_insertion("hi", "h\ni", 1), 2);
    }
}
