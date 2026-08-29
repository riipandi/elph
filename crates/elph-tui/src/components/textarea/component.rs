//! iocraft [`Textarea`] — thin shell around [`TextareaState`] + direct render.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::TextareaProps;
use super::input::handle_textarea_terminal_event;
use super::input::{TextareaInputContext, TextareaInputResult};
use super::layout::{layout_cursor_for_viewport, layout_metrics_from_wrapped, layout_textarea_measured};
use super::state::{TextareaState, selection_display_rows};
use crate::clipboard::{
    ClipboardNotice, ImageAttachment, copy_to_clipboard, read_from_clipboard, remove_image_attachments,
    save_clipboard_image,
};
use crate::components::scroll_bar::VerticalScrollbar;
use crate::components::theme::resolve_ui_theme;
use crate::input_prefix::{InputPrefixKind, PromptPrefixConfig, absorb_inline_triggers};
use crate::input_prefix::{backspace_trigger_kind, try_consume_trigger};
use crate::text_editing::{is_file_picker_dismiss_key, is_file_picker_nav_key, is_palette_capture_key};
use crate::text_input_layout::WrappedTextLayout;
use crate::text_input_layout::overlay_editor_wrap_width;
use crate::text_input_layout::update_scroll_offset;
use iocraft::prelude::*;

/// Key event plus flushed editor buffer for parent palette handlers.
#[derive(Debug, Clone)]
pub struct PaletteKeyInput {
    pub code: KeyCode,
    pub kind: KeyEventKind,
    pub modifiers: KeyModifiers,
    pub draft: String,
    pub cursor: usize,
}

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
fn prefix_config_enabled(config: Option<PromptPrefixConfig>) -> bool {
    config.is_some_and(|config| config.enabled)
}

fn try_prefix_terminal_event(
    event: TerminalEvent,
    config: Option<PromptPrefixConfig>,
    input_prefix_kind: Option<Ref<InputPrefixKind>>,
    body: &str,
) -> Option<TextareaInputResult> {
    if !prefix_config_enabled(config) {
        return None;
    }
    let TerminalEvent::Key(KeyEvent {
        code, kind, modifiers, ..
    }) = event
    else {
        return None;
    };
    if kind == KeyEventKind::Release || !modifiers.is_empty() {
        return None;
    }
    let mut kind_ref = input_prefix_kind?;
    let config = config?;
    match code {
        KeyCode::Char(ch) => try_consume_trigger(kind_ref.get(), body, ch, config.enabled).map(|next| {
            kind_ref.set(next);
            TextareaInputResult::Consumed
        }),
        KeyCode::Backspace => backspace_trigger_kind(kind_ref.get(), body, config.enabled).map(|next| {
            kind_ref.set(next);
            TextareaInputResult::Consumed
        }),
        _ => None,
    }
}

fn sync_prefix_draft(
    config: Option<PromptPrefixConfig>,
    input_prefix_kind: Option<Ref<InputPrefixKind>>,
    editor: &mut Ref<TextareaState>,
    value: &mut State<String>,
    live_draft: Option<Ref<String>>,
) -> bool {
    let Some(config) = config.filter(|config| config.enabled) else {
        return false;
    };
    let Some(mut kind_ref) = input_prefix_kind else {
        return false;
    };
    let mut ed = editor.write();
    let text = ed.text.clone();
    let (next_kind, next_body) = absorb_inline_triggers(kind_ref.get(), &text, &config);
    let mut changed = false;
    if next_kind != kind_ref.get() {
        kind_ref.set(next_kind);
        changed = true;
    }
    if next_body != text {
        ed.sync_external(&next_body);
        changed = true;
    }
    if changed {
        if let Some(mut live) = live_draft {
            live.set(next_body.clone());
        }
        value.set(next_body);
    }
    changed
}

fn sync_editor_from_parent(ed: &mut TextareaState, external: &str, has_focus: bool) {
    if has_focus {
        if TextareaState::should_sync_focused_external(&ed.text, external) {
            ed.sync_external(external);
        }
        return;
    }
    ed.sync_external(external);
}

fn prune_removed_image_attachments(text: &str, attachments: &mut Vec<ImageAttachment>) {
    let mut removed = Vec::new();
    attachments.retain(|attachment| {
        let marker = format!("[Image #{}]", attachment.id);
        if text.contains(&marker) {
            true
        } else {
            removed.push(attachment.clone());
            false
        }
    });
    remove_image_attachments(&removed);
}

fn next_image_id(text: &str) -> usize {
    (1..)
        .find(|id| {
            let marker = format!("[Image #{id}]");
            !text.contains(&marker)
        })
        .unwrap_or(usize::MAX)
}

#[derive(Debug)]
enum BackgroundPasteResult {
    Image(ImageAttachment),
    Text(String),
    Unsupported,
    Failed(String),
}

fn apply_palette_draft_to_editor(ed: &mut TextareaState, external: &str, forced_cursor: Option<usize>) {
    ed.sync_external_with_cursor(external, forced_cursor);
}

fn mirror_editor_buffer(
    ed: &TextareaState,
    live_draft: Option<Ref<String>>,
    live_cursor: Option<Ref<usize>>,
    prompt_editor_mirror: Option<Ref<(String, usize)>>,
) {
    if let Some(mut live) = live_draft {
        live.set(ed.text.clone());
    }
    if let Some(mut cursor_ref) = live_cursor {
        cursor_ref.set(ed.cursor);
    }
    if let Some(mut mirror) = prompt_editor_mirror {
        mirror.set((ed.text.clone(), ed.cursor));
    }
}

/// Multiline text input with optional external state.
#[component]
pub fn Textarea(props: &mut TextareaProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let internal = hooks.use_state(|| props.initial_value.clone());
    let mut value = props.value.unwrap_or(internal);
    let suppress_enter_newline = props.suppress_enter_newline;
    let slash_palette_active = props.slash_palette_active;
    let file_picker_active = props.file_picker_active;
    let styled_content = props.styled_content;
    let live_cursor = props.live_cursor;
    let force_palette_sync = props.force_palette_sync;
    let force_clear = props.force_clear;
    let live_draft = props.live_draft;
    let prefix_config = props.prefix_config;
    let input_prefix_kind = props.input_prefix_kind;
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
    let image_paste_result = hooks.use_ref(|| Arc::new(Mutex::new(None::<BackgroundPasteResult>)));
    let image_paste_busy = hooks.use_ref(|| false);
    let mut generation = hooks.use_state(|| 0u32);
    let on_submit = props.on_submit.take();
    let on_escape = props.on_escape.take();
    let on_file_picker_key = props.on_file_picker_key.take();
    let file_picker_key_handled = props.file_picker_key_handled;
    let prompt_editor_mirror = props.prompt_editor_mirror;
    let clipboard_toast = props.clipboard_toast;
    let image_attachment_dir = props.image_attachment_dir.clone();
    let image_attachments = props.image_attachments;
    let supports_images = props.supports_images;

    {
        let result_slot = image_paste_result.read().clone();
        let mut image_paste_busy = image_paste_busy;
        let mut editor = editor;
        let mut value = value;
        let mut layout_cache = layout_cache;
        let mut viewport_cache = viewport_cache;
        let mut generation = generation;
        hooks.use_future(async move {
            loop {
                smol::Timer::after(Duration::from_millis(32)).await;
                let result = result_slot.lock().ok().and_then(|mut slot| slot.take());
                let Some(result) = result else {
                    continue;
                };
                image_paste_busy.set(false);
                match result {
                    BackgroundPasteResult::Image(saved) => {
                        let Some(mut attachment_ref) = image_attachments else {
                            remove_image_attachments(&[saved]);
                            if let Some(mut toast) = clipboard_toast {
                                toast.set(Some(ClipboardNotice::ImagePasteFailed {
                                    detail: "the prompt is no longer available".to_string(),
                                }));
                            }
                            continue;
                        };
                        let mut attachments = attachment_ref.write();
                        let next_id = {
                            let ed = editor.read();
                            next_image_id(&ed.text)
                        };
                        let attachment = ImageAttachment { id: next_id, ..saved };
                        attachments.push(attachment.clone());
                        editor.write().insert_image_marker(next_id);
                        let ed = editor.read();
                        value.set(ed.text.clone());
                        if let Some(mut live) = live_draft {
                            live.set(ed.text.clone());
                        }
                        if let Some(mut cursor_ref) = live_cursor {
                            cursor_ref.set(ed.cursor);
                        }
                        if let Some(mut mirror) = prompt_editor_mirror {
                            mirror.set((ed.text.clone(), ed.cursor));
                        }
                        if let Some(mut toast) = clipboard_toast {
                            toast.set(Some(ClipboardNotice::ImagePasted { id: next_id }));
                        }
                        layout_cache.set(None);
                        viewport_cache.set(None);
                        generation.set(generation.get().wrapping_add(1));
                    }
                    BackgroundPasteResult::Text(text) => {
                        let mut ed = editor.write();
                        ed.apply_paste(&text);
                        if let Some(mut live) = live_draft {
                            live.set(ed.text.clone());
                        }
                        if let Some(mut cursor_ref) = live_cursor {
                            cursor_ref.set(ed.cursor);
                        }
                        if let Some(mut mirror) = prompt_editor_mirror {
                            mirror.set((ed.text.clone(), ed.cursor));
                        }
                        value.set(ed.text.clone());
                        if let Some(mut toast) = clipboard_toast {
                            toast.set(Some(ClipboardNotice::ImagePasteText));
                        }
                        layout_cache.set(None);
                        viewport_cache.set(None);
                        generation.set(generation.get().wrapping_add(1));
                    }
                    BackgroundPasteResult::Unsupported => {
                        if let Some(mut toast) = clipboard_toast {
                            toast.set(Some(ClipboardNotice::ImageInputUnsupported));
                        }
                    }
                    BackgroundPasteResult::Failed(detail) => {
                        if let Some(mut toast) = clipboard_toast {
                            toast.set(Some(ClipboardNotice::ImagePasteFailed { detail }));
                        }
                    }
                }
            }
        });
    }

    // Flush raw paste-burst buffers after a typing gap so rapid keys are not lost when the
    // user stops (merge used to run only on the *next* keypress).
    {
        let mut editor = editor;
        let mut paste_burst = paste_burst;
        let mut value = value;
        let mut generation = generation;
        let mut layout_cache = layout_cache;
        let mut viewport_cache = viewport_cache;
        hooks.use_future(async move {
            use crate::text_editing::PASTE_BURST_WINDOW;
            loop {
                // smol timer: iocraft polls futures on smol (mock + some hosts lack a Tokio RT).
                smol::Timer::after(Duration::from_millis(32)).await;
                if !paste_burst.read().active {
                    continue;
                }
                let Some(last) = *last_key_at.read() else {
                    continue;
                };
                if last.elapsed() < PASTE_BURST_WINDOW {
                    continue;
                }
                let mut ed = editor.write();
                let mut burst = paste_burst.write();
                if !super::input::flush_idle_burst(&mut ed, &mut burst) {
                    continue;
                }
                let text = ed.text.clone();
                let cursor = ed.cursor;
                drop(burst);
                drop(ed);
                if let Some(mut live) = live_draft {
                    live.set(text.clone());
                }
                if let Some(mut cursor_ref) = live_cursor {
                    cursor_ref.set(cursor);
                }
                if let Some(mut mirror) = prompt_editor_mirror {
                    mirror.set((text.clone(), cursor));
                }
                value.set(text);
                layout_cache.set(None);
                viewport_cache.set(None);
                generation.set(generation.get().wrapping_add(1));
            }
        });
    }

    if !has_focus {
        pending_esc.set(false);
    }

    {
        let mut ed = editor.write();
        if force_clear.is_some_and(|signal| signal.get()) {
            if let Some(mut attachments) = image_attachments {
                let pending = attachments.read().clone();
                remove_image_attachments(&pending);
                attachments.set(Vec::new());
            }
            ed.clear_after_submit();
            if let Some(mut signal) = force_clear {
                signal.set(false);
            }
            if let Some(mut kind) = input_prefix_kind {
                kind.set(InputPrefixKind::Default);
            }
            if let Some(mut live) = live_draft {
                live.set(String::new());
            }
            value.set(String::new());
            layout_cache.set(None);
            viewport_cache.set(None);
            generation.set(generation.get().wrapping_add(1));
        }
        if force_palette_sync.is_some_and(|signal| signal.get()) {
            let external = value.read().clone();
            let forced_cursor = live_cursor.as_ref().map(|cursor_ref| cursor_ref.get());
            apply_palette_draft_to_editor(&mut ed, &external, forced_cursor);
            if let Some(mut signal) = force_palette_sync {
                signal.set(false);
            }
        } else {
            sync_editor_from_parent(&mut ed, &value.read(), has_focus);
        }
    }
    if let Some(mut attachments) = image_attachments {
        let text = editor.read().text.clone();
        prune_removed_image_attachments(&text, &mut attachments.write());
    }

    let h_pad = if show_border { 2 } else { 0 };
    let inner_width = props.width.saturating_sub(h_pad);
    // Snapshot layout while the editor read guard is held, then drop it before registering
    // terminal hooks — otherwise key dispatch can block on `editor.write()` during render.
    let (
        layout,
        rendered_text,
        selection_rows,
        text_wrap,
        content_scroll_offset,
        cursor_display_row,
        cursor_col_clamped,
    ) = {
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
        let next_scroll =
            update_scroll_offset(scroll_row.get(), cursor_row, layout.viewport_height, layout.content_rows);
        if next_scroll != scroll_row.get() {
            scroll_row.set(next_scroll);
        }
        if let Some(mut cursor_ref) = live_cursor {
            cursor_ref.set(ed.cursor);
        }
        if let Some(mut mirror) = prompt_editor_mirror {
            mirror.set((ed.text.clone(), ed.cursor));
        }
        let selection = ed.selection_range();
        let has_active_selection = selection.is_some();
        let use_viewport_slice = styled_content.is_none() && ed.text.len() >= VIEWPORT_SLICE_MIN_CHARS;
        let visible_row_count = layout.viewport_height.saturating_add(1);
        // Prefer segmented selection paint when a range is active (a11y: high-contrast invert).
        let selection_rows = if has_active_selection && styled_content.is_none() {
            Some(selection_display_rows(
                &ed.text,
                &wrapped,
                selection,
                next_scroll,
                visible_row_count,
            ))
        } else {
            None
        };
        let (rendered_text, text_wrap, content_scroll_offset, cursor_display_row) = if selection_rows.is_some() {
            // Selection path paints MixedText rows; plain fallback unused.
            (String::new(), TextWrap::NoWrap, 0i32, cursor_row.saturating_sub(next_scroll))
        } else if use_viewport_slice {
            let slice_key = (ed.text.clone(), next_scroll, visible_row_count);
            let content =
                if viewport_cache.read().as_ref().is_some_and(|c| {
                    c.text == slice_key.0 && c.scroll_row == slice_key.1 && c.viewport_rows == slice_key.2
                }) {
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
            // Use WrappedTextLayout for consistent wrapping — iocraft's built-in
            // TextWrap::Wrap may compute line breaks differently (e.g. reserve
            // one column for the cursor), desyncing cursor position from text.
            // Scroll-aware: generate the viewport slice from next_scroll.
            let content = wrapped.display_text_for_row_range(&ed.text, next_scroll, layout.viewport_height);
            (content, TextWrap::NoWrap, 0i32, cursor_row.saturating_sub(next_scroll))
        };
        let cursor_col_clamped = if layout.input_width > 0 {
            cursor_col.min(layout.input_width.saturating_sub(1))
        } else {
            cursor_col
        };
        (
            layout,
            rendered_text,
            selection_rows,
            text_wrap,
            content_scroll_offset,
            cursor_display_row,
            cursor_col_clamped,
        )
    };

    hooks.use_terminal_events({
        let mut editor = editor;
        let mut value = value;
        let mut generation = generation;
        let mut on_submit = on_submit;
        let mut on_escape = on_escape;
        let mut on_file_picker_key = on_file_picker_key;
        let mut pending_esc = pending_esc;
        let mut paste_burst = paste_burst;
        let mut last_key_at = last_key_at;
        let submit_on_enter = props.submit_on_enter;
        let image_attachment_dir = image_attachment_dir.clone();
        let image_paste_result = image_paste_result.read().clone();
        let mut image_paste_busy = image_paste_busy;
        let input_width = layout.input_width;
        move |event| {
            if !has_focus {
                return;
            }

            {
                let ed = editor.read();
                if let Some(mut mirror) = prompt_editor_mirror {
                    mirror.set((ed.text.clone(), ed.cursor));
                }
            }

            if file_picker_active.is_some_and(|active| active.get())
                && !on_file_picker_key.is_default()
                && let TerminalEvent::Key(KeyEvent {
                    code, kind, modifiers, ..
                }) = &event
                && *kind != KeyEventKind::Release
                && (is_palette_capture_key(*code, *kind, *modifiers)
                    || is_file_picker_nav_key(*code, *kind, *modifiers)
                    || is_file_picker_dismiss_key(*code, *kind, *modifiers))
            {
                let mut ed = editor.write();
                let mut burst = paste_burst.write();
                if burst.active {
                    super::input::flush_idle_burst(&mut ed, &mut burst);
                }
                let input = PaletteKeyInput {
                    code: *code,
                    kind: *kind,
                    modifiers: *modifiers,
                    draft: ed.text.clone(),
                    cursor: ed.cursor,
                };
                drop(burst);
                drop(ed);
                on_file_picker_key(input);
                if file_picker_key_handled.is_some_and(|handled| handled.get()) {
                    if force_palette_sync.is_some_and(|signal| signal.get()) {
                        let external = value.read().clone();
                        let forced_cursor = live_cursor.as_ref().map(|cursor_ref| cursor_ref.get());
                        {
                            let mut ed = editor.write();
                            apply_palette_draft_to_editor(&mut ed, &external, forced_cursor);
                            mirror_editor_buffer(&ed, live_draft, live_cursor, prompt_editor_mirror);
                        }
                        value.set(external);
                        if let Some(mut signal) = force_palette_sync {
                            signal.set(false);
                        }
                        layout_cache.set(None);
                        viewport_cache.set(None);
                    }
                    generation.set(generation.get().wrapping_add(1));
                    return;
                }
            }

            let mut esc = pending_esc.get();
            let text_before = editor.read().text.clone();
            if let Some(result) =
                try_prefix_terminal_event(event.clone(), prefix_config, input_prefix_kind, &text_before)
            {
                if result == TextareaInputResult::Consumed {
                    generation.set(generation.get().wrapping_add(1));
                }
                return;
            }
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
                        slash_palette_active,
                        file_picker_active,
                        pending_esc: &mut esc,
                        paste_burst: &mut burst,
                        last_key_at: &mut last,
                        on_escape: &mut on_escape,
                    },
                )
            };
            pending_esc.set(esc);

            let sync_live_draft = |text: &str, cursor: usize| {
                if let Some(mut live) = live_draft {
                    live.set(text.to_string());
                }
                if let Some(mut cursor_ref) = live_cursor {
                    cursor_ref.set(cursor);
                }
            };

            match result {
                TextareaInputResult::Submit(draft) => {
                    if !supports_images && image_attachments.is_some_and(|attachments| !attachments.read().is_empty()) {
                        if let Some(mut toast) = clipboard_toast {
                            toast.set(Some(ClipboardNotice::ImageInputUnsupported));
                        }
                        return;
                    }
                    sync_live_draft(&draft, editor.read().cursor);
                    if !on_submit.is_default() {
                        on_submit(draft);
                    }
                    let mut ed = editor.write();
                    ed.clear_after_submit();
                    if let Some(mut attachments) = image_attachments {
                        let pending = attachments.read().clone();
                        remove_image_attachments(&pending);
                        attachments.set(Vec::new());
                    }
                    sync_live_draft("", 0);
                    value.set(String::new());
                    if let Some(mut kind) = input_prefix_kind {
                        kind.set(InputPrefixKind::Default);
                    }
                    layout_cache.set(None);
                    viewport_cache.set(None);
                    generation.set(generation.get().wrapping_add(1));
                }
                TextareaInputResult::Changed => {
                    // Always mirror parent draft — including during short raw-burst typing so
                    // shell palette / chrome never re-sync from a stale empty draft.
                    {
                        let ed = editor.read();
                        let text = ed.text.clone();
                        sync_live_draft(&text, ed.cursor);
                        value.set(text);
                    }
                    if sync_prefix_draft(prefix_config, input_prefix_kind, &mut editor, &mut value, live_draft) {
                        viewport_cache.set(None);
                    }
                    if let Some(mut suppress) = suppress_enter_newline {
                        suppress.set(false);
                    }
                    if text_before != editor.read().text {
                        viewport_cache.set(None);
                    }
                    generation.set(generation.get().wrapping_add(1));
                }
                TextareaInputResult::Consumed => {
                    // Bulk raw-burst (long paste): keep parent draft at least as long as the
                    // visible editor so an idle flush cannot race with an empty parent value.
                    if file_picker_active.is_some_and(|active| active.get())
                        || slash_palette_active.is_some_and(|active| active.get())
                        || paste_burst.read().active
                    {
                        let ed = editor.read();
                        sync_live_draft(&ed.text, ed.cursor);
                        value.set(ed.text.clone());
                    }
                }
                TextareaInputResult::Yank(selected) => {
                    let notice = match copy_to_clipboard(&selected) {
                        Ok(()) => ClipboardNotice::selection_copied(selected.chars().count()),
                        Err(err) => {
                            log::warn!("selection yank failed: {err}");
                            ClipboardNotice::failed(err.to_string())
                        }
                    };
                    // State wakes MainShell so the toast is painted on the next frame.
                    if let Some(mut toast) = clipboard_toast {
                        toast.set(Some(notice));
                    }
                    {
                        let ed = editor.read();
                        sync_live_draft(&ed.text, ed.cursor);
                    }
                    generation.set(generation.get().wrapping_add(1));
                }
                TextareaInputResult::PasteImage => {
                    if image_paste_busy.get() {
                        return;
                    }
                    image_paste_busy.set(true);
                    if let Some(mut toast) = clipboard_toast {
                        toast.set(Some(ClipboardNotice::ImagePasting));
                    }
                    if let Ok(mut slot) = image_paste_result.lock() {
                        *slot = None;
                    }
                    let attachment_dir = image_attachment_dir.clone();
                    let result_slot = image_paste_result.clone();
                    smol::spawn(async move {
                        let result = smol::unblock(move || {
                            if !supports_images {
                                return match read_from_clipboard() {
                                    Ok(text) if !text.is_empty() => BackgroundPasteResult::Text(text),
                                    Ok(_) | Err(_) => BackgroundPasteResult::Unsupported,
                                };
                            }
                            if let Some(dir) = attachment_dir.as_deref()
                                && let Ok(image) = save_clipboard_image(dir, 0)
                            {
                                return BackgroundPasteResult::Image(image);
                            }
                            match read_from_clipboard() {
                                Ok(text) if !text.is_empty() => BackgroundPasteResult::Text(text),
                                Ok(_) => {
                                    BackgroundPasteResult::Failed("clipboard does not contain an image".to_string())
                                }
                                Err(err) => BackgroundPasteResult::Failed(err.to_string()),
                            }
                        })
                        .await;
                        if let Ok(mut slot) = result_slot.lock() {
                            *slot = Some(result);
                        }
                    })
                    .detach();
                }
                TextareaInputResult::Ignored => {}
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

    let theme = resolve_ui_theme(&hooks, props.theme);
    let scrollbar_style = props.scrollbar_style.unwrap_or_else(|| theme.scrollbar_style());
    let outer_viewport = layout.viewport_height;
    let text_color = props.text_color.unwrap_or_else(|| theme.input_text_color(has_focus));
    let cursor_color = props.cursor_color.unwrap_or_else(|| theme.input_cursor_color());
    let border_inset = if show_border { theme.input_inset() } else { 0 };

    element! {
        View(
            width: props.width,
            height: outer_viewport,
            flex_shrink: 0f32,
            position: Position::Relative,
            overflow: Overflow::Hidden,
            border_style: border_style,
            border_color: theme.input_border_color(has_focus),
            padding_left: border_inset,
            padding_right: border_inset,
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
                    #( {
                        let body: AnyElement<'static> = if let Some(rows) = selection_rows.as_ref() {
                            // High-contrast selection: inverted spans (color + invert — not color alone).
                            let row_elements: Vec<AnyElement<'static>> = rows
                                .iter()
                                .map(|segs| {
                                    let contents: Vec<MixedTextContent> = segs
                                        .iter()
                                        .map(|(text, selected)| {
                                            let mut content = MixedTextContent::new(text.clone()).color(text_color);
                                            if *selected {
                                                content = content.invert();
                                            }
                                            content
                                        })
                                        .collect();
                                    element! {
                                        MixedText(
                                            contents: contents,
                                            wrap: TextWrap::NoWrap,
                                        )
                                    }
                                    .into()
                                })
                                .collect();
                            element! {
                                View(flex_direction: FlexDirection::Column, width: layout.input_width) {
                                    #(row_elements)
                                }
                            }
                            .into()
                        } else {
                            element! {
                                Text(
                                    content: rendered_text.clone(),
                                    wrap: text_wrap,
                                    color: text_color,
                                )
                            }
                            .into()
                        };
                        body
                    })
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
                            scroll_offset: scroll_row.get(),
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
    fn next_image_id_reuses_first_free_id_in_current_prompt() {
        assert_eq!(next_image_id("[Image #2]"), 1);
        assert_eq!(next_image_id(""), 1);
        assert_eq!(next_image_id("[Image #1] [Image #2]"), 3);
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
    fn forced_palette_sync_applies_trailing_space_completion() {
        let mut ed = TextareaState::from_text("/goal".into());
        ed.sync_external("/goal ");
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
