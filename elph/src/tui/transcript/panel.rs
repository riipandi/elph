//! Scrollable transcript panel with sticky user prompts.

use std::time::Duration;

use elph_tui::{
    active_sticky_user_message_index, layout_sticky_header, scroll_view_down, scroll_view_max_offset, scroll_view_up,
    sticky_source_bubble_suppressed, transcript_bubble_inner_width,
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
/// Markdown partition/parse tick — stay off the hot path of shell chrome ticks.
const MARKDOWN_DEBOUNCE_MS: u64 = 120;
const MAX_MARKDOWN_PARSE_JOBS_PER_TICK: usize = 1;

#[derive(Clone, Default, Props)]
pub struct TranscriptPanelProps {
    pub screen_width: u16,
    pub messages: Option<State<Vec<TranscriptMessage>>>,
    /// Bumped when `messages` changes — avoids re-hashing on scroll-only re-renders.
    /// Also written by clickable process headers when expand/collapse toggles.
    pub messages_revision: Option<State<u64>>,
    pub sticky_scroll: bool,
    pub has_focus: bool,
}

struct TranscriptRenderCache {
    messages_revision: u64,
    markdown_layout_revision: u64,
    screen_width: u16,
    row_layouts: Vec<elph_tui::TranscriptRowLayout>,
    is_sticky_prompt: Vec<bool>,
    /// Fingerprinted row-count slots — survives revision bumps for unchanged prefix messages.
    layout_cache: IncrementalLayoutCache,
}

#[component]
pub fn TranscriptPanel(props: &TranscriptPanelProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let scroll_handle = hooks.use_ref_default::<ScrollViewHandle>();
    let mut render_cache = hooks.use_ref(|| None::<TranscriptRenderCache>);
    let mut last_sticky_rows = hooks.use_ref(|| 0u16);
    let scroll_generation = hooks.use_state(|| 0u32);
    let mut markdown_layout_revision = hooks.use_state(|| 0u64);
    let empty_messages = hooks.use_state(Vec::<TranscriptMessage>::new);
    let mut messages_state = props.messages.unwrap_or(empty_messages);
    let mut screen_width_ref = hooks.use_ref(|| props.screen_width);
    screen_width_ref.set(props.screen_width);

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(MARKDOWN_DEBOUNCE_MS)).await;
            let width = screen_width_ref.get();

            let (partition_changed, jobs) = {
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
                let mut msgs = messages_state.write();
                if apply_markdown_parse_result(&mut msgs, &job, document) {
                    parsed = true;
                }
            }

            if partition_changed || parsed {
                markdown_layout_revision.set(markdown_layout_revision.get().wrapping_add(1));
            }
        }
    });

    let messages = messages_state.read();
    let _scroll_generation = scroll_generation.get();
    let messages_revision_value = props.messages_revision.map(|s| s.get()).unwrap_or(0);
    let cache_key = (messages_revision_value, markdown_layout_revision.get(), props.screen_width);

    if render_cache.read().as_ref().is_none_or(|c| {
        c.messages_revision != cache_key.0 || c.markdown_layout_revision != cache_key.1 || c.screen_width != cache_key.2
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
    // Near-bottom follow: treat as pinned even if the handle briefly unpins when height grows.
    let near_bottom = {
        let max_off = scroll_view_max_offset(layout_content_u16, scroll_zone);
        let offset = handle.scroll_offset();
        auto_pinned || offset >= max_off.saturating_sub(2)
    };

    // Sticky inset uses last frame's height only when sticky is actually active (manual scroll).
    let sticky_inset = if near_bottom { 0 } else { *last_sticky_rows.read() };
    let panel_viewport = scroll_zone.saturating_add(sticky_inset);

    let sticky_idx = props
        .sticky_scroll
        .then(|| {
            active_sticky_user_message_index(
                row_layouts,
                is_sticky_prompt,
                handle.scroll_offset(),
                near_bottom,
                panel_viewport,
            )
        })
        .flatten()
        .filter(|&idx| idx < messages.len() && is_sticky_prompt.get(idx).copied().unwrap_or(false));

    let effective_scroll_offset = if near_bottom {
        // Align window to layout total, not lagging ScrollView metrics.
        layout_content_rows.saturating_sub(scroll_zone as u32)
    } else {
        handle.scroll_offset().max(0) as u32
    };
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
    let panel_height = panel_viewport;
    let sticky_header = sticky_idx.and_then(|idx| {
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
    let sticky_rows = sticky_header.as_ref().map(|header| header.height).unwrap_or(0);
    last_sticky_rows.set(sticky_rows);
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

    let transcript_focused = props.has_focus;
    hooks.use_terminal_events({
        let mut scroll_handle = scroll_handle;
        let mut scroll_generation = scroll_generation;
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
                scroll_generation.set(scroll_generation.get().wrapping_add(1));
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
                        scrollbar: true,
                        scrollbar_thumb_color: SCROLLBAR_THUMB,
                        scrollbar_track_color: SCROLLBAR_TRACK,
                        keyboard_scroll: Some(false),
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
            }
        }
    }
}
