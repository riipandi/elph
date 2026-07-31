//! Scrollable transcript panel with sticky user prompts.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use elph_tui::{
    StickyHeaderLayout, active_sticky_user_message_index, layout_sticky_header, scroll_view_down,
    scroll_view_max_offset, scroll_view_up, sticky_source_bubble_suppressed, transcript_bubble_inner_width,
};
use iocraft::prelude::*;

use super::card::{CollapsibleToggleCtx, build_transcript_bubbles_windowed, transcript_sticky_overlay};
use super::layout::{IncrementalLayoutCache, layout_transcript_rows_cached};
use super::markdown::{
    apply_markdown_parse_result, collect_markdown_parse_jobs, parse_markdown_on_worker, partition_assistant_markdown,
};
use super::types::TranscriptMessage;
use crate::tui::focus::transcript_nav_key;
use crate::tui::theme::{BORDER_MUTED, SCROLLBAR_THUMB, SCROLLBAR_TRACK, TRANSCRIPT_BORDER_FOCUSED};

const TRANSCRIPT_SCROLL_STEP: i32 = 3;
/// Minimum scrollable lines below a sticky user prompt.
const STICKY_MIN_SCROLL_ROWS: u16 = 3;
const MARKDOWN_DEBOUNCE_MS: u64 = 120;
/// Slower markdown parse ticks during active streaming to keep TUI responsive.
const MARKDOWN_STREAMING_DEBOUNCE_MS: u64 = 400;
const MAX_MARKDOWN_PARSE_JOBS_PER_TICK: usize = 1;

#[derive(Default, Props)]
pub struct TranscriptPanelProps {
    pub screen_width: u16,
    pub messages: Option<State<Vec<TranscriptMessage>>>,
    /// Bumped when `messages` changes — avoids re-hashing on scroll-only re-renders.
    /// Also written by clickable process headers when expand/collapse toggles.
    pub messages_revision: Option<State<u64>>,
    pub sticky_scroll: bool,
    pub has_focus: bool,
    /// When false, mouse wheel is ignored (e.g. while a modal dialog owns scroll).
    /// Defaults to `true` so the transcript still scrolls while the prompt has focus.
    pub mouse_scroll: Option<bool>,
    /// Native terminal text-select mode — hide the scrollbar so it does not interfere
    /// with drag-to-select.
    pub text_select_mode: bool,
    /// Set by shell when a streaming response is active — slows markdown parse ticks.
    /// None/Some(false) = idle debounce (120ms), Some(true) = streaming debounce (400ms).
    pub streaming_active: Option<bool>,
    /// Arc<RwLock> messages — decouples panel from shell's State dirt chain.
    /// Panel reads/writes this directly instead of the `messages` State.
    pub messages_arc: Option<Arc<RwLock<Vec<TranscriptMessage>>>>,
    /// CachedTranscript — hybrid in-memory + disk-backed transcript store.
    /// When set, the markdown future reads from this instead of `messages_arc`.
    pub transcript: Option<Arc<RwLock<super::cache::CachedTranscript>>>,
    /// Click handler for subagent status lines. Fires with `(agent_id, title)` when
    /// a subagent status row is clicked.
    pub on_subagent_click: Option<HandlerMut<'static, (String, String)>>,
}

struct TranscriptRenderCache {
    messages_revision: u64,
    markdown_layout_revision: u64,
    screen_width: u16,
    streaming_content_fp: u64,
    row_layouts: Vec<elph_tui::TranscriptRowLayout>,
    is_sticky_prompt: Vec<bool>,
    /// Fingerprinted row-count slots — survives revision bumps for unchanged prefix messages.
    layout_cache: IncrementalLayoutCache,
}

/// Cached sticky header layout to avoid WrappedTextLayout construction per frame.
#[derive(Default)]
struct StickyHeaderCache {
    key: (Option<usize>, u64, u16), // (idx, content_hash_or_panel_height, screen_width)
    result: Option<StickyHeaderLayout>,
}

#[component]
pub fn TranscriptPanel(props: &TranscriptPanelProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let scroll_handle = hooks.use_ref_default::<ScrollViewHandle>();
    let mut render_cache = hooks.use_ref(|| None::<TranscriptRenderCache>);
    let mut cached_sticky_rows = hooks.use_ref(|| (None::<usize>, 0u16, 0u16)); // (idx, width, rows)
    let scroll_offset_override = hooks.use_ref(|| None::<u32>);
    // Cache for full StickyHeaderLayout — avoids WrappedTextLayout per frame.
    let mut sticky_header_cache = hooks.use_ref(StickyHeaderCache::default);
    let mut markdown_layout_revision = hooks.use_state(|| 0u64);
    let empty_messages = hooks.use_state(Vec::<TranscriptMessage>::new);
    let mut messages_state = props.messages.unwrap_or(empty_messages);
    // Arc<RwLock> decouples panel from shell's State dirt chain.
    // When available, panel reads/writes the arc directly.
    let messages_arc = props.messages_arc.clone();
    let mut screen_width_ref = hooks.use_ref(|| props.screen_width);
    screen_width_ref.set(props.screen_width);
    let mut near_bottom_sticky = hooks.use_ref(|| true);
    let mut last_committed_bottom = hooks.use_ref(|| 0u32);
    let mut is_streaming = hooks.use_ref(|| false);
    is_streaming.set(props.streaming_active.unwrap_or(false));

    hooks.use_future(async move {
        let messages_arc = messages_arc; // moved into future
        loop {
            let debounce = if is_streaming.get() {
                MARKDOWN_STREAMING_DEBOUNCE_MS
            } else {
                MARKDOWN_DEBOUNCE_MS
            };
            tokio::time::sleep(Duration::from_millis(debounce)).await;
            let width = screen_width_ref.get();

            // Use messages_arc when available (decoupled from shell State).
            // Fall back to messages_state for backward compat.
            let (partition_changed, jobs) = if let Some(ref arc) = messages_arc {
                let mut msgs = arc.write().expect("transcript arc lock");
                let changed = partition_assistant_markdown(&mut msgs, width);
                let jobs = collect_markdown_parse_jobs(&msgs);
                (changed, jobs)
            } else {
                let mut msgs = messages_state.write();
                let changed = partition_assistant_markdown(&mut msgs, width);
                let jobs = collect_markdown_parse_jobs(&msgs);
                (changed, jobs)
            };

            let mut parsed = false;
            for job in jobs.into_iter().take(MAX_MARKDOWN_PARSE_JOBS_PER_TICK) {
                let source = job.source.clone();
                let document = match tokio::task::spawn_blocking(move || parse_markdown_on_worker(&source)).await {
                    Ok(doc) => doc,
                    Err(_) => continue,
                };
                if let Some(ref arc) = messages_arc {
                    let mut msgs = arc.write().expect("transcript arc lock");
                    if apply_markdown_parse_result(&mut msgs, &job, document) {
                        parsed = true;
                    }
                } else {
                    let mut msgs = messages_state.write();
                    if apply_markdown_parse_result(&mut msgs, &job, document) {
                        parsed = true;
                    }
                }
            }

            if partition_changed || parsed {
                markdown_layout_revision.set(markdown_layout_revision.get().wrapping_add(1));
            }
        }
    });

    let messages = messages_state.read();
    let messages_revision_value = props.messages_revision.map(|s| s.get()).unwrap_or(0);

    // Streaming content changes every tick but `messages_revision` only updates at the
    // publish interval. Detect in-place content growth (e.g. assistant/tool streaming)
    // so the render cache invalidates before the next publish tick.
    let streaming_content_fp = messages
        .last()
        .map(|m| {
            if m.duration_secs.is_none() {
                m.content.len() as u64
            } else {
                0
            }
        })
        .unwrap_or(0);

    let cache_key = (
        messages_revision_value,
        markdown_layout_revision.get(),
        props.screen_width,
        streaming_content_fp,
    );

    if render_cache.read().as_ref().is_none_or(|c| {
        c.messages_revision != cache_key.0
            || c.markdown_layout_revision != cache_key.1
            || c.screen_width != cache_key.2
            || c.streaming_content_fp != cache_key.3
    }) {
        let mut layout_cache = render_cache
            .read()
            .as_ref()
            .map(|c| c.layout_cache.clone())
            .unwrap_or_default();
        let row_layouts = layout_transcript_rows_cached(&messages, props.screen_width, &mut layout_cache);
        let is_sticky_prompt: Vec<_> = messages.iter().map(|m| m.style.is_sticky_prompt()).collect();
        render_cache.set(Some(TranscriptRenderCache {
            messages_revision: cache_key.0,
            markdown_layout_revision: cache_key.1,
            screen_width: cache_key.2,
            streaming_content_fp: cache_key.3,
            row_layouts,
            is_sticky_prompt,
            layout_cache,
        }));
    }

    let cache = render_cache.read();
    let cached = match cache.as_ref() {
        Some(c) => c,
        None => {
            // Should be populated above; avoid panicking the whole TUI on a cache miss.
            return element! {
                View(
                    width: props.screen_width,
                    flex_grow: 1f32,
                    flex_shrink: 1f32,
                    min_height: 0,
                    overflow: Overflow::Hidden,
                )
            };
        }
    };
    let row_layouts = &cached.row_layouts;
    let is_sticky_prompt = &cached.is_sticky_prompt;

    let handle = scroll_handle.read();
    let scroll_zone = handle.viewport_height().max(1);
    // Layout-measured content height (stable across frames). Prefer this over ScrollView's
    // previous-frame content_height so windowing/sticky don't thrash while streaming.
    let layout_content_rows = row_layouts
        .last()
        .map(|layout| layout.start_row.saturating_add(layout.row_count))
        .unwrap_or(0);
    let layout_content_u16 = layout_content_rows.min(u16::MAX as u32) as u16;

    let auto_pinned = handle.is_auto_scroll_pinned();
    // Near-bottom with hysteresis to prevent flicker during streaming.
    let max_off = scroll_view_max_offset(layout_content_u16, scroll_zone);
    let raw_offset = scroll_offset_override
        .read()
        .map(|o| o as i32)
        .unwrap_or_else(|| handle.scroll_offset());
    // Only leave near_bottom when user has scrolled meaningfully away from bottom.
    // Threshold: 6 rows (2 scroll steps) above bottom.
    let is_near = auto_pinned || raw_offset >= max_off.saturating_sub(6);
    let near_bottom = if auto_pinned {
        near_bottom_sticky.set(true);
        true
    } else if is_near && *near_bottom_sticky.read() {
        // Stay near-bottom if was near-bottom (hysteresis hold).
        true
    } else if raw_offset < max_off.saturating_sub(6) {
        near_bottom_sticky.set(false);
        false
    } else {
        *near_bottom_sticky.read()
    };

    // Compute effective_scroll_offset BEFORE sticky_idx so sticky uses the correct
    // visual offset (not the raw handle offset, which stays at 0 during auto-scroll).
    // Also used for bubble windowing — must reflect the actual scroll position
    // (not the content bottom) so windowed mounting covers in-viewport rows.
    let effective_scroll_offset = if near_bottom {
        let bottom = layout_content_rows.saturating_sub(scroll_zone as u32);
        last_committed_bottom.set(bottom);
        bottom
    } else {
        raw_offset.max(0) as u32
    };

    // Compute sticky_idx using effective_scroll_offset so it correctly detects
    // scrolled-off prompts even while auto-scroll keeps the viewport at the bottom.
    // While pinned to the bottom the sticky bar would hide the top rows of the latest
    // card (viewport shrinks by sticky_rows and the card shifts up behind it) — the
    // card then looks clipped mid-line. Only show the sticky bar once the user actually
    // scrolls up (near_bottom = false); auto-scroll shows the latest card in full.
    let sticky_idx = props
        .sticky_scroll
        .then(|| {
            if near_bottom {
                return None;
            }
            active_sticky_user_message_index(
                row_layouts,
                is_sticky_prompt,
                effective_scroll_offset as i32,
                near_bottom,
                scroll_zone,
            )
        })
        .flatten()
        .filter(|&idx| idx < messages.len() && is_sticky_prompt.get(idx).copied().unwrap_or(false));

    // Memoize sticky_rows with key (sticky_idx, screen_width).
    // Uses scroll_zone as panel_height to avoid circular dep.
    let sticky_rows = {
        let (cached_idx, cached_width, cached_rows) = *cached_sticky_rows.read();
        if cached_idx == sticky_idx && cached_width == props.screen_width {
            cached_rows
        } else {
            let rows = sticky_idx
                .and_then(|idx| messages.get(idx))
                .and_then(|msg| {
                    let style = msg.style;
                    layout_sticky_header(
                        &msg.content,
                        transcript_bubble_inner_width(props.screen_width, style.horizontal_padding())
                            .saturating_sub(style.content_chrome_cols())
                            .max(1),
                        style.sticky_bubble_padding_rows(),
                        scroll_zone,
                        STICKY_MIN_SCROLL_ROWS,
                    )
                    .map(|h| h.height)
                })
                .unwrap_or(0);
            cached_sticky_rows.set((sticky_idx, props.screen_width, rows));
            rows
        }
    };
    // panel_viewport uses stable sticky_rows (no one-frame lag).
    let sticky_inset = sticky_rows;
    let panel_viewport = scroll_zone.saturating_add(sticky_inset);
    let panel_height = panel_viewport;

    let suppress_sticky_source =
        sticky_source_bubble_suppressed(row_layouts, sticky_idx, effective_scroll_offset as i32, scroll_zone);
    let toggle = match (props.messages, props.messages_revision) {
        (Some(messages), Some(messages_revision)) => Some(CollapsibleToggleCtx {
            messages,
            messages_revision,
        }),
        _ => None,
    };
    // Only mount bubbles that can affect the viewport (+ overscan/tail). Off-screen history
    // becomes fixed-height spacers so spinner/chrome re-renders stay cheap on long logs.
    let bubbles = build_transcript_bubbles_windowed(
        props.screen_width,
        &messages,
        row_layouts,
        effective_scroll_offset,
        scroll_zone as u32,
        suppress_sticky_source,
        toggle,
    );
    // Cached sticky header — avoids WrappedTextLayout per frame.
    let sticky_header = {
        let mut cache_guard = sticky_header_cache.write();
        let meta_hash: Option<u64> = sticky_idx.and_then(|idx| {
            let msg = messages.get(idx)?;
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            msg.content.hash(&mut hasher);
            Some(hasher.finish())
        });
        let cache_key: Option<(Option<usize>, u64, u16)> = sticky_idx
            .zip(meta_hash)
            .map(|(idx, hash)| (Some(idx), hash, props.screen_width));
        if let Some(key) = cache_key {
            if cache_guard.key == key {
                cache_guard.result.clone()
            } else {
                let result = sticky_idx.and_then(|idx| {
                    let message = messages.get(idx)?;
                    if !message.style.is_sticky_prompt() {
                        return None;
                    }
                    let style = message.style;
                    layout_sticky_header(
                        &message.content,
                        transcript_bubble_inner_width(props.screen_width, style.horizontal_padding())
                            .saturating_sub(style.content_chrome_cols())
                            .max(1),
                        style.sticky_bubble_padding_rows(),
                        panel_height,
                        STICKY_MIN_SCROLL_ROWS,
                    )
                });
                cache_guard.key = key;
                cache_guard.result = result.clone();
                result
            }
        } else {
            cache_guard.key = (None, 0, 0);
            cache_guard.result = None;
            None
        }
    };
    let sticky_overlay = sticky_idx.zip(sticky_header.as_ref()).and_then(|(idx, header)| {
        let message = messages.get(idx)?;
        let style = message.style;
        let inner_width = transcript_bubble_inner_width(props.screen_width, style.horizontal_padding())
            .saturating_sub(style.content_chrome_cols())
            .max(1);
        Some(transcript_sticky_overlay(
            header.height,
            inner_width,
            message,
            &header.display_text,
        ))
    });
    let min_content_height = scroll_zone;

    // In text-select mode: hide the iocraft scrollbar AND explicitly write spaces over
    // the scrollbar column so stale canvas characters are not copied during native
    // terminal drag-to-select.
    let show_scrollbar = !props.text_select_mode;
    // Cache clear_scrollbar_text — regenerates only when scroll_zone changes.
    let mut cached_clear_text = hooks.use_ref(|| (0u16, String::new()));
    let clear_scrollbar_text = if props.text_select_mode && scroll_zone > 0 {
        let cached = {
            let (cached_zone, ref cached_str) = *cached_clear_text.read();
            if cached_zone == scroll_zone {
                Some(cached_str.clone())
            } else {
                None
            }
        };
        if let Some(cached) = cached {
            Some(cached)
        } else {
            let text = if scroll_zone == 1 {
                " ".to_string()
            } else {
                " \n".repeat((scroll_zone - 1) as usize) + " "
            };
            cached_clear_text.set((scroll_zone, text.clone()));
            Some(text)
        }
    } else {
        None
    };

    let transcript_focused = props.has_focus;
    hooks.use_terminal_events({
        let mut scroll_handle = scroll_handle;
        let mut scroll_offset_override = scroll_offset_override;
        move |event| {
            let TerminalEvent::Key(KeyEvent {
                code, kind, modifiers, ..
            }) = event
            else {
                return;
            };
            if kind == KeyEventKind::Release {
                return;
            }

            let scroll_step = match code {
                KeyCode::PageUp | KeyCode::PageDown => TRANSCRIPT_SCROLL_STEP.saturating_mul(3),
                _ => TRANSCRIPT_SCROLL_STEP,
            };

            let scrolled = if transcript_focused && transcript_nav_key(code, kind, modifiers) {
                match code {
                    KeyCode::Up | KeyCode::PageUp => {
                        scroll_view_up(&mut scroll_handle.write(), scroll_step);
                        true
                    }
                    KeyCode::Down | KeyCode::PageDown => {
                        scroll_view_down(&mut scroll_handle.write(), scroll_step);
                        true
                    }
                    KeyCode::Home => {
                        scroll_handle.write().scroll_to(0);
                        true
                    }
                    KeyCode::End => {
                        let (content_height, viewport_height) = {
                            let h = scroll_handle.read();
                            (h.content_height(), h.viewport_height())
                        };
                        scroll_handle
                            .write()
                            .scroll_to(scroll_view_max_offset(content_height, viewport_height));
                        true
                    }
                    _ => false,
                }
            } else if modifiers.contains(KeyModifiers::SHIFT)
                && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META)
                && matches!(code, KeyCode::Up | KeyCode::Down)
            {
                match code {
                    KeyCode::Up => {
                        scroll_view_up(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP);
                        true
                    }
                    KeyCode::Down => {
                        scroll_view_down(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP);
                        true
                    }
                    _ => false,
                }
            } else {
                false
            };
            if scrolled {
                scroll_offset_override.set(Some(scroll_handle.read().scroll_offset().max(0) as u32));
            }
        }
    });

    element! {
        View(
            width: props.screen_width,
            flex_grow: 1f32,
            flex_shrink: 1f32,
            min_height: 0,
            overflow: Overflow::Hidden,
            border_style: BorderStyle::Single,
            border_edges: Edges::Top,
            border_color: if props.has_focus {
                TRANSCRIPT_BORDER_FOCUSED
            } else {
                BORDER_MUTED
            },
            // One blank row under the transcript before StatusRow. Transcript flex-shrinks
            // to absorb it so the footer stays on-screen (root uses Overflow::Hidden).
            margin_bottom: 1,
        ) {
            View(
                width: 100pct,
                height: 100pct,
                position: Position::Relative,
                overflow: Overflow::Hidden,
            ) {
                View(
                    position: Position::Absolute,
                    top: sticky_rows,
                    left: 0,
                    right: 0,
                    bottom: 0,
                    overflow: Overflow::Hidden,
                ) {
                    ScrollView(
                        handle: Some(scroll_handle),
                        scroll_step: TRANSCRIPT_SCROLL_STEP as u16,
                        scrollbar: show_scrollbar,
                        scrollbar_thumb_color: SCROLLBAR_THUMB,
                        scrollbar_track_color: SCROLLBAR_TRACK,
                        keyboard_scroll: Some(false),
                        mouse_scroll: Some(props.mouse_scroll.unwrap_or(true)),
                        auto_scroll: true,
                    ) {
                        View(
                            width: props.screen_width,
                            min_height: min_content_height,
                            background_color: Color::Reset,
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::End,
                            align_items: AlignItems::Baseline,
                            padding_bottom: 0,
                            padding_left: 1,
                            padding_right: 1,
                            gap: 0,
                        ) {
                            #(bubbles)
                        }
                    }
                }
                #(sticky_overlay)
                // Overwrite the scrollbar column with spaces so stale characters
                // from the iocraft ScrollViewScrollbar canvas are never copied
                // during native terminal text selection.
                #(clear_scrollbar_text.as_ref().map(|text| {
                    element! {
                        View(
                            position: Position::Absolute,
                            right: 0,
                            top: sticky_rows as i16,
                            width: 1,
                            height: scroll_zone,
                            overflow: Overflow::Hidden,
                        ) {
                            Text(content: text.as_str())
                        }
                    }
                }))
            }
        }
    }
}
