//! Scrollable transcript panel with sticky user prompts.

mod message;

use elph_tui::{
    effective_scroll_offset, layout_transcript_rows, scroll_view_down, scroll_view_up, sticky_user_message_index,
    transcript_text_width,
};
use iocraft::prelude::*;

use super::theme::{BORDER_MUTED, SCROLLBAR_TRACK};

pub use message::{TranscriptMessage, TranscriptStyle, seed_transcript_messages};

use message::{transcript_message_bubble, transcript_sticky_overlay};

const TRANSCRIPT_SCROLL_STEP: i32 = 2;

#[derive(Clone, Default, Props)]
pub struct TranscriptPanelProps {
    pub screen_width: u16,
    pub messages: Vec<TranscriptMessage>,
    pub sticky_scroll: bool,
}

#[component]
pub fn TranscriptPanel(props: &TranscriptPanelProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let scroll_handle = hooks.use_ref_default::<ScrollViewHandle>();
    // At least viewport tall so short transcripts stay bottom-anchored; grows with content.
    let min_content_height = scroll_handle.read().viewport_height().max(1);
    let handle = scroll_handle.read();
    let scroll_offset = effective_scroll_offset(
        handle.scroll_offset(),
        handle.is_auto_scroll_pinned(),
        handle.content_height(),
        handle.viewport_height(),
    );
    let row_layouts = layout_transcript_rows(
        &props.messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
        transcript_text_width(props.screen_width),
        1,
    );
    let is_user: Vec<_> = props.messages.iter().map(|m| m.style.is_user()).collect();
    let sticky_idx = props
        .sticky_scroll
        .then(|| sticky_user_message_index(&row_layouts, &is_user, scroll_offset))
        .flatten();
    let bubbles: Vec<_> = props
        .messages
        .iter()
        .map(|message| transcript_message_bubble(props.screen_width, message))
        .collect();

    hooks.use_terminal_events({
        let mut scroll_handle = scroll_handle;
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
            match code {
                KeyCode::Up => scroll_view_up(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP),
                KeyCode::Down => scroll_view_down(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP),
                _ => {}
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
                    scrollbar_thumb_color: BORDER_MUTED,
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
                        padding_top: 0,
                        padding_bottom: 0,
                        padding_left: 1,
                        padding_right: 1,
                        gap: 1,
                    ) {
                        #(bubbles)
                    }
                }
                #(if let Some(idx) = sticky_idx {
                    Some(transcript_sticky_overlay(props.screen_width, &props.messages[idx]))
                } else {
                    None
                })
            }
        }
    }
}
