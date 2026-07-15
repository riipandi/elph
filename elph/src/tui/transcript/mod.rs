//! Scrollable transcript panel with sticky user prompts.

mod message;

use elph_tui::{
    TranscriptRowLayout, active_sticky_user_message_index, layout_sticky_header, scroll_view_down, scroll_view_up,
    transcript_bubble_inner_width, wrapped_transcript_row_count,
};
use iocraft::prelude::*;

use super::theme::{BORDER_MUTED, SCROLLBAR_THUMB, SCROLLBAR_TRACK};

pub use message::{
    TranscriptMessage, TranscriptStyle, format_tool_card_content, format_tool_card_result, seed_transcript_messages,
};

use message::{build_transcript_bubbles, transcript_sticky_overlay};

const TRANSCRIPT_SCROLL_STEP: i32 = 3;
/// Minimum scrollable lines below a sticky user prompt.
const STICKY_MIN_SCROLL_ROWS: u16 = 3;

#[derive(Clone, Default, Props)]
pub struct TranscriptPanelProps {
    pub screen_width: u16,
    pub messages: Option<State<Vec<TranscriptMessage>>>,
    /// Bumped when `messages` changes — avoids re-hashing on scroll-only re-renders.
    pub messages_revision: u64,
    pub sticky_scroll: bool,
}

struct TranscriptRenderCache {
    revision: u64,
    screen_width: u16,
    row_layouts: Vec<TranscriptRowLayout>,
    is_sticky_prompt: Vec<bool>,
}

#[component]
pub fn TranscriptPanel(props: &TranscriptPanelProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let scroll_handle = hooks.use_ref_default::<ScrollViewHandle>();
    let mut render_cache = hooks.use_ref(|| None::<TranscriptRenderCache>);
    let scroll_generation = hooks.use_state(|| 0u32);
    let empty_messages = hooks.use_state(Vec::<TranscriptMessage>::new);
    let messages_state = props.messages.unwrap_or(empty_messages);
    let messages = messages_state.read();
    let _scroll_generation = scroll_generation.get();
    let cache_key = (props.messages_revision, props.screen_width);

    if render_cache
        .read()
        .as_ref()
        .is_none_or(|c| c.revision != cache_key.0 || c.screen_width != cache_key.1)
    {
        let row_layouts = layout_transcript_rows(&messages, props.screen_width);
        let is_sticky_prompt: Vec<_> = messages.iter().map(|m| m.style.is_sticky_prompt()).collect();
        render_cache.set(Some(TranscriptRenderCache {
            revision: cache_key.0,
            screen_width: cache_key.1,
            row_layouts,
            is_sticky_prompt,
        }));
    }

    let cache = render_cache.read();
    let cached = cache.as_ref().expect("transcript render cache");
    let row_layouts = &cached.row_layouts;
    let is_sticky_prompt = &cached.is_sticky_prompt;
    let bubbles = build_transcript_bubbles(props.screen_width, &messages);

    let handle = scroll_handle.read();
    let scroll_viewport = handle.viewport_height().max(1);
    let min_content_height = scroll_viewport;
    let sticky_idx = props
        .sticky_scroll
        .then(|| {
            active_sticky_user_message_index(
                row_layouts,
                is_sticky_prompt,
                handle.scroll_offset(),
                handle.is_auto_scroll_pinned(),
            )
        })
        .flatten();
    let panel_height = scroll_viewport;
    let sticky_header = sticky_idx.and_then(|idx| {
        if !messages[idx].style.is_sticky_prompt() {
            return None;
        }
        layout_sticky_header(
            &messages[idx].content,
            transcript_bubble_inner_width(props.screen_width, messages[idx].style.horizontal_padding()),
            messages[idx].style.sticky_bubble_padding_rows(),
            panel_height,
            STICKY_MIN_SCROLL_ROWS,
        )
    });
    let sticky_rows = sticky_header.as_ref().map(|h| h.height).unwrap_or(0);

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
            if kind == KeyEventKind::Release || !modifiers.contains(KeyModifiers::SHIFT) {
                return;
            }
            let scrolled = match code {
                KeyCode::Up => {
                    scroll_view_up(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP);
                    true
                }
                KeyCode::Down => {
                    scroll_view_down(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP);
                    true
                }
                _ => false,
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
            border_color: BORDER_MUTED,
            margin_bottom: 1,
        ) {
            View(
                width: 100pct,
                height: 100pct,
                position: Position::Relative,
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
                        padding_top: sticky_rows,
                        padding_bottom: 0,
                        padding_left: 1,
                        padding_right: 1,
                        gap: 0,
                    ) {
                        #(bubbles)
                    }
                }
                #(if let (Some(idx), Some(header)) = (sticky_idx, sticky_header.as_ref()) {
                    Some(transcript_sticky_overlay(
                        header.height,
                        &messages[idx],
                        &header.display_text,
                    ))
                } else {
                    None
                })
            }
        }
    }
}

fn layout_transcript_rows(messages: &[TranscriptMessage], screen_width: u16) -> Vec<TranscriptRowLayout> {
    let mut layouts = Vec::with_capacity(messages.len());
    let mut cursor = 0u32;
    for (index, message) in messages.iter().enumerate() {
        let wrap_width = transcript_bubble_inner_width(screen_width, message.style.horizontal_padding());
        let row_count = wrapped_transcript_row_count(&message.content, wrap_width) as u32;
        layouts.push(TranscriptRowLayout {
            start_row: cursor,
            row_count,
        });
        cursor = cursor.saturating_add(row_count);
        if index + 1 < messages.len() {
            let next_style = messages.get(index + 1).map(|m| m.style);
            cursor = cursor.saturating_add(message.style.entry_gap_after(next_style) as u32);
        }
    }
    layouts
}
