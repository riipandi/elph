//! Terminal hook for single-line [`TextInput`] wire shortcuts + paste.

use iocraft::prelude::*;

use crate::paste::apply_paste_at_cursor;

use super::wire::wire_edit_handle_key;

// TODO: Review iocraft PR#183 (on_key_event prop) to properly handle keyboard shortcuts in TextInput.
// The current implementation uses wire_edit_handle_key with multiline=true to match textarea behavior,
// but word deletion shortcuts (Cmd+Backspace, Opt+Backspace) still don't work in single-line Input.
// PR#183 adds an on_key_event prop that would allow us to override default keyboard handling properly.
// See: https://github.com/ccbrown/iocraft/pull/183

/// Wire GUI shortcuts and bracketed paste into a single-line [`TextInput`].
pub fn wire_input_shortcuts(
    hooks: &mut Hooks,
    has_focus: bool,
    mut value: State<String>,
    input_handle: Ref<TextInputHandle>,
) {
    let pending_esc = hooks.use_ref(|| false);

    hooks.use_terminal_events({
        let mut input_handle = input_handle;
        let mut pending_esc = pending_esc;
        move |event| {
            if !has_focus {
                return;
            }

            if let TerminalEvent::Paste(data) = event {
                let prev = value.read().clone();
                let cursor = input_handle.read().cursor_offset();
                let (text, cursor) = apply_paste_at_cursor(&prev, cursor, &data);
                input_handle.write().set_cursor_offset(cursor);
                value.set(text);
                return;
            }

            let TerminalEvent::Key(KeyEvent {
                code, kind, modifiers, ..
            }) = event
            else {
                return;
            };

            let prev = value.read().clone();
            let mut text = prev.clone();
            let mut esc = pending_esc.get();
            let mut handle = input_handle.write();

            // Try wire shortcuts first (use multiline=true to match textarea behavior)
            if wire_edit_handle_key(code, kind, modifiers, true, &mut esc, &mut text, &mut handle) {
                drop(handle);
                pending_esc.set(esc);
                if text != prev {
                    value.set(text);
                }
                return;
            }

            // If wire shortcuts didn't handle it, let TextInput handle default behavior
            drop(handle);
            pending_esc.set(esc);
        }
    });
}
