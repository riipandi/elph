//! iocraft [`Textarea`] — thin shell around [`TextareaState`] + direct render.

use std::sync::Arc;

use super::TextareaProps;
use super::input::{TextareaInputContext, TextareaInputResult, handle_textarea_terminal_event};
use super::layout::{layout_cursor_for_viewport, layout_metrics_from_wrapped, layout_textarea_measured};
use super::state::TextareaState;
use crate::components::scroll_bar::{ScrollbarStyle, VerticalScrollbar};
use crate::text_input_layout::overlay_editor_wrap_width;
use crate::text_input_layout::{WrappedTextLayout, update_scroll_offset};
use iocraft::prelude::*;

/// Prefer viewport slicing above this source length (paste-sized buffers).
const VIEWPORT_SLICE_MIN_CHARS: usize = 2_048;

#[derive(Clone)]
struct TextareaLayoutCache {
    text: String,
    inner_width: u16,
    min_height: u16,
    max_height: Option<u16>,
    wrapped: Arc<WrappedTextLayout>,
}

#[derive(Clone, PartialEq, Eq)]
struct ViewportRenderCache {
    text: String,
    scroll_row: u16,
    viewport_rows: u16,
    content: String,
}

fn resolve_textarea_layout(
    cache: &mut Ref<Option<TextareaLayoutCache>>,
    text: &str,
    layout_cursor: usize,
    inner_width: u16,
    min_height: u16,
    max_height: Option<u16>,
) -> (super::layout::TextareaLayout, Arc<WrappedTextLayout>) {
    let wrap_width = overlay_editor_wrap_width(inner_width);
    let dims_match = |cached: &TextareaLayoutCache| {
        cached.inner_width == inner_width && cached.min_height == min_height && cached.max_height == max_height
    };

    let cached_snapshot = cache.read().clone();
    if let Some(cached) = cached_snapshot.as_ref() {
        if dims_match(cached) && cached.text == text {
            let layout =
                layout_metrics_from_wrapped(&cached.wrapped, text, layout_cursor, inner_width, min_height, max_height);
            return (layout, Arc::clone(&cached.wrapped));
        }
        if dims_match(cached) {
            if let Some(wrapped) = WrappedTextLayout::try_extend_suffix(&cached.wrapped, &cached.text, text, wrap_width)
            {
                let layout =
                    layout_metrics_from_wrapped(&wrapped, text, layout_cursor, inner_width, min_height, max_height);
                let wrapped = Arc::new(wrapped);
                cache.set(Some(TextareaLayoutCache {
                    text: text.to_string(),
                    inner_width,
                    min_height,
                    max_height,
                    wrapped: Arc::clone(&wrapped),
                }));
                return (layout, wrapped);
            }
            if let Some(wrapped) =
                WrappedTextLayout::try_truncate_suffix(&cached.wrapped, &cached.text, text, wrap_width)
            {
                let layout =
                    layout_metrics_from_wrapped(&wrapped, text, layout_cursor, inner_width, min_height, max_height);
                let wrapped = Arc::new(wrapped);
                cache.set(Some(TextareaLayoutCache {
                    text: text.to_string(),
                    inner_width,
                    min_height,
                    max_height,
                    wrapped: Arc::clone(&wrapped),
                }));
                return (layout, wrapped);
            }
        }
    }

    let (layout, wrapped) = layout_textarea_measured(text, layout_cursor, inner_width, min_height, max_height);
    let wrapped = Arc::new(wrapped);
    cache.set(Some(TextareaLayoutCache {
        text: text.to_string(),
        inner_width,
        min_height,
        max_height,
        wrapped: Arc::clone(&wrapped),
    }));
    (layout, wrapped)
}

/// Pull parent draft when unfocused, or when the parent explicitly sets non-empty text.
///
/// While focused, an empty external draft is normal — we no longer mirror every keystroke into
/// parent state for performance. Clearing the editor on empty external would wipe live input.
fn sync_editor_from_parent(ed: &mut TextareaState, external: &str, has_focus: bool) {
    if has_focus {
        if TextareaState::should_sync_focused_external(&ed.text, external) {
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
    let mut value = props.value.unwrap_or(internal);
    let suppress_enter_newline = props.suppress_enter_newline;
    let force_clear = props.force_clear;
    let live_draft = props.live_draft;
    let has_focus = props.has_focus;
    let min_height = props.min_height.max(1);
    let show_border = props.show_border.unwrap_or(true);

    let mut editor = hooks.use_ref(|| TextareaState::from_text(value.read().clone()));
    let mut pending_esc = hooks.use_ref(|| false);
    let paste_burst = hooks.use_ref(crate::paste::PasteBurstState::default);
    let last_key_at = hooks.use_ref(|| None::<std::time::Instant>);
    let mut scroll_row = hooks.use_ref(|| 0u16);
    let mut layout_cache = hooks.use_ref(|| None::<TextareaLayoutCache>);
    let mut viewport_cache = hooks.use_ref(|| None::<ViewportRenderCache>);
    let mut generation = hooks.use_state(|| 0u32);
    let on_submit = props.on_submit.take();
    let on_escape = props.on_escape.take();

    if !has_focus {
        pending_esc.set(false);
    }

    {
        let mut ed = editor.write();
        if force_clear.is_some_and(|signal| signal.get()) {
            ed.clear_after_submit();
            if let Some(mut signal) = force_clear {
                signal.set(false);
            }
            if let Some(mut live) = live_draft {
                live.set(String::new());
            }
            value.set(String::new());
            layout_cache.set(None);
            viewport_cache.set(None);
            generation.set(generation.get().wrapping_add(1));
        }
        sync_editor_from_parent(&mut ed, &value.read(), has_focus);
    }

    let h_pad = if show_border { 2 } else { 0 };
    let inner_width = props.width.saturating_sub(h_pad);
    let ed = editor.read();
    let _generation = generation.get();
    let layout_cursor = layout_cursor_for_viewport(&ed.text, ed.cursor);
    let (layout, wrapped) = resolve_textarea_layout(
        &mut layout_cache,
        &ed.text,
        layout_cursor,
        inner_width,
        min_height,
        props.max_height,
    );
    let display_cursor = layout_cursor_for_viewport(&ed.text, ed.cursor);
    let (cursor_row, cursor_col) = wrapped.row_column_for_offset(&ed.text, display_cursor);
    let next_scroll = update_scroll_offset(scroll_row.get(), cursor_row, layout.viewport_height, layout.content_rows);
    if next_scroll != scroll_row.get() {
        scroll_row.set(next_scroll);
    }
    let use_viewport_slice = ed.text.len() >= VIEWPORT_SLICE_MIN_CHARS;
    let visible_row_count = layout.viewport_height.saturating_add(1);
    let (rendered_text, text_wrap, content_scroll_offset, cursor_display_row) = if use_viewport_slice {
        let slice_key = (ed.text.clone(), next_scroll, visible_row_count);
        let content = if viewport_cache
            .read()
            .as_ref()
            .is_some_and(|c| c.text == slice_key.0 && c.scroll_row == slice_key.1 && c.viewport_rows == slice_key.2)
        {
            viewport_cache.read().as_ref().expect("viewport cache").content.clone()
        } else {
            let content = wrapped.display_text_for_row_range(&ed.text, next_scroll, visible_row_count);
            viewport_cache.set(Some(ViewportRenderCache {
                text: slice_key.0,
                scroll_row: slice_key.1,
                viewport_rows: slice_key.2,
                content: content.clone(),
            }));
            content
        };
        (content, TextWrap::NoWrap, 0i32, cursor_row.saturating_sub(next_scroll))
    } else {
        viewport_cache.set(None);
        (ed.text.clone(), TextWrap::Wrap, next_scroll as i32, cursor_row)
    };
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
        let mut on_escape = on_escape;
        let mut pending_esc = pending_esc;
        let mut paste_burst = paste_burst;
        let mut last_key_at = last_key_at;
        let submit_on_enter = props.submit_on_enter;
        let input_width = layout.input_width;
        move |event| {
            let mut esc = pending_esc.get();
            let text_before = editor.read().text.clone();
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
                        on_escape: &mut on_escape,
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
                    layout_cache.set(None);
                    viewport_cache.set(None);
                    generation.set(generation.get().wrapping_add(1));
                }
                TextareaInputResult::Changed => {
                    if !paste_burst.read().active {
                        let text = editor.read().text.clone();
                        sync_live_draft(&text);
                        value.set(text);
                    }
                    if let Some(mut suppress) = suppress_enter_newline {
                        suppress.set(false);
                    }
                    if text_before != editor.read().text {
                        viewport_cache.set(None);
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
                    top: -content_scroll_offset,
                    left: 0,
                    width: layout.input_width,
                ) {
                    #(if has_focus {
                        Some(element! {
                            View(
                                position: Position::Absolute,
                                top: cursor_display_row,
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
                        content: rendered_text,
                        wrap: text_wrap,
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
    fn focused_stale_parent_does_not_clobber_post_completion_typing() {
        let mut ed = TextareaState::from_text("/goal args".into());
        sync_editor_from_parent(&mut ed, "/goal ", true);
        assert_eq!(ed.text, "/goal args");
    }

    #[test]
    fn focused_stale_parent_does_not_restore_deleted_suffix() {
        let mut ed = TextareaState::from_text("/goal".into());
        sync_editor_from_parent(&mut ed, "/goal ", true);
        assert_eq!(ed.text, "/goal");
    }

    #[test]
    fn focused_parent_slash_completion_still_syncs() {
        let mut ed = TextareaState::from_text("/go".into());
        sync_editor_from_parent(&mut ed, "/goal ", true);
        assert_eq!(ed.text, "/goal ");
        assert_eq!(ed.cursor, 6);
    }

    #[test]
    fn unfocused_parent_still_syncs_empty_draft() {
        let mut ed = TextareaState::from_text("hello".into());
        sync_editor_from_parent(&mut ed, "", false);
        assert!(ed.text.is_empty());
    }

    #[test]
    fn clear_after_submit_wipes_buffer() {
        let mut ed = TextareaState::from_text("draft".into());
        ed.clear_after_submit();
        assert!(ed.text.is_empty());
        assert_eq!(ed.cursor, 0);
    }
}
