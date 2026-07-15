//! iocraft [`Textarea`] — thin shell around [`TextareaState`] + direct render.

use super::TextareaProps;
use super::input::{TextareaInputContext, TextareaInputResult, handle_textarea_terminal_event};
use super::layout::{layout_cursor_for_viewport, layout_textarea_measured};
use super::state::TextareaState;
use crate::components::scroll_bar::{ScrollbarStyle, VerticalScrollbar};
use crate::text_input_layout::update_scroll_offset;
use iocraft::prelude::*;

/// Pull parent draft when unfocused, or when the parent explicitly sets non-empty text.
///
/// While focused, an empty external draft is normal — we no longer mirror every keystroke into
/// parent state for performance. Clearing the editor on empty external would wipe live input.
fn sync_editor_from_parent(ed: &mut TextareaState, external: &str, has_focus: bool) {
    if has_focus {
        if !external.is_empty() && ed.text != external {
            ed.sync_external(external);
        }
        return;
    }
    ed.sync_external(external);
}

/// Multiline text input with optional external state.
#[component]
pub fn Textarea(props: &mut TextareaProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let internal = hooks.use_state(|| props.initial_value.clone());
    let value = props.value.unwrap_or(internal);
    let suppress_enter_newline = props.suppress_enter_newline;
    let live_draft = props.live_draft;
    let has_focus = props.has_focus;
    let min_height = props.min_height.max(1);
    let show_border = props.show_border.unwrap_or(true);

    let mut editor = hooks.use_ref(|| TextareaState::from_text(value.read().clone()));
    let pending_esc = hooks.use_ref(|| false);
    let paste_burst = hooks.use_ref(crate::paste::PasteBurstState::default);
    let last_key_at = hooks.use_ref(|| None::<std::time::Instant>);
    let mut scroll_row = hooks.use_ref(|| 0u16);
    let generation = hooks.use_state(|| 0u32);
    let on_submit = props.on_submit.take();

    {
        let mut ed = editor.write();
        sync_editor_from_parent(&mut ed, &value.read(), has_focus);
    }

    let h_pad = if show_border { 2 } else { 0 };
    let inner_width = props.width.saturating_sub(h_pad);
    let ed = editor.read();
    let _generation = generation.get();
    let text = ed.text.clone();
    let layout_cursor = layout_cursor_for_viewport(&ed.text, ed.cursor);
    let (layout, wrapped) =
        layout_textarea_measured(&ed.text, layout_cursor, inner_width, min_height, props.max_height);
    let display_cursor = layout_cursor_for_viewport(&ed.text, ed.cursor);
    let (cursor_row, cursor_col) = wrapped.row_column_for_offset(&ed.text, display_cursor);
    let next_scroll = update_scroll_offset(scroll_row.get(), cursor_row, layout.viewport_height, layout.content_rows);
    scroll_row.set(next_scroll);
    let cursor_col_clamped = if layout.input_width > 0 {
        cursor_col.min(layout.input_width.saturating_sub(1))
    } else {
        cursor_col
    };

    hooks.use_terminal_events({
        let mut editor = editor;
        let mut value = value;
        let mut generation = generation;
        let mut on_submit = on_submit;
        let mut pending_esc = pending_esc;
        let mut paste_burst = paste_burst;
        let mut last_key_at = last_key_at;
        let submit_on_enter = props.submit_on_enter;
        let input_width = layout.input_width;
        move |event| {
            let mut esc = pending_esc.get();
            let result = {
                let mut ed = editor.write();
                let mut burst = paste_burst.write();
                let mut last = last_key_at.write();
                handle_textarea_terminal_event(
                    event,
                    &mut ed,
                    TextareaInputContext {
                        has_focus,
                        input_width,
                        submit_on_enter,
                        suppress_enter_newline,
                        pending_esc: &mut esc,
                        paste_burst: &mut burst,
                        last_key_at: &mut last,
                    },
                )
            };
            pending_esc.set(esc);

            let sync_live_draft = |text: &str| {
                if let Some(mut live) = live_draft {
                    live.set(text.to_string());
                }
            };

            match result {
                TextareaInputResult::Submit(draft) => {
                    sync_live_draft(&draft);
                    if !on_submit.is_default() {
                        on_submit(draft);
                    }
                    let mut ed = editor.write();
                    ed.clear_after_submit();
                    sync_live_draft("");
                    value.set(String::new());
                    generation.set(generation.get().wrapping_add(1));
                }
                TextareaInputResult::Changed => {
                    sync_live_draft(&editor.read().text);
                    if let Some(mut suppress) = suppress_enter_newline {
                        suppress.set(false);
                    }
                    generation.set(generation.get().wrapping_add(1));
                }
                TextareaInputResult::Consumed | TextareaInputResult::Ignored => {}
            }
        }
    });

    let border_style = if show_border && has_focus {
        BorderStyle::Round
    } else if show_border {
        BorderStyle::Single
    } else {
        BorderStyle::None
    };

    let scrollbar_style = props.scrollbar_style.unwrap_or_else(ScrollbarStyle::dark);
    let outer_viewport = layout.viewport_height;
    let text_color = props.text_color.unwrap_or(Color::Grey);
    let cursor_color = props.cursor_color.unwrap_or(Color::DarkGrey);

    element! {
        View(
            width: props.width,
            height: outer_viewport,
            flex_shrink: 0f32,
            position: Position::Relative,
            overflow: Overflow::Hidden,
            border_style: border_style,
            border_color: Color::DarkGrey,
            padding_left: if show_border { 1 } else { 0 },
            padding_right: if show_border { 1 } else { 0 },
        ) {
            View(
                position: Position::Absolute,
                top: 0,
                left: 0,
                width: layout.input_width,
                height: outer_viewport,
                overflow: Overflow::Hidden,
            ) {
                View(
                    position: Position::Absolute,
                    top: -(next_scroll as i32),
                    left: 0,
                    width: layout.input_width,
                ) {
                    #(if has_focus {
                        Some(element! {
                            View(
                                position: Position::Absolute,
                                top: cursor_row,
                                left: cursor_col_clamped,
                                width: 1,
                                height: 1,
                                background_color: cursor_color,
                            )
                        })
                    } else {
                        None
                    })
                    Text(
                        content: text,
                        wrap: TextWrap::Wrap,
                        color: text_color,
                    )
                }
            }
            #(if layout.show_scrollbar {
                Some(element! {
                    View(
                        position: Position::Absolute,
                        top: 0,
                        right: 0,
                        width: 1,
                        height: outer_viewport,
                    ) {
                        VerticalScrollbar(
                            viewport_height: outer_viewport,
                            content_height: layout.content_rows,
                            scroll_offset: next_scroll,
                            style: Some(scrollbar_style),
                        )
                    }
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
    fn focused_empty_parent_does_not_clear_live_input() {
        let mut ed = TextareaState::from_text("hello".into());
        sync_editor_from_parent(&mut ed, "", true);
        assert_eq!(ed.text, "hello");
    }

    #[test]
    fn unfocused_parent_still_syncs_empty_draft() {
        let mut ed = TextareaState::from_text("hello".into());
        sync_editor_from_parent(&mut ed, "", false);
        assert!(ed.text.is_empty());
    }
}
