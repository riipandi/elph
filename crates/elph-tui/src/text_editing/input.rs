//! Terminal hook for single-line [`TextInput`] wire shortcuts + paste.

use iocraft::prelude::*;

use crate::paste::apply_paste_at_cursor;

use super::wire::wire_edit_handle_key;

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
            if !wire_edit_handle_key(code, kind, modifiers, false, &mut esc, &mut text, &mut handle) {
                return;
            }
            drop(handle);
            pending_esc.set(esc);
            if text != prev {
                value.set(text);
            }
        }
    });
}
