//! Scrollable transcript panel with optional sticky user prompts.

use crate::common::transcript::style::STICKY_MIN_SCROLL_ROWS;
use crate::common::transcript::{
    TRANSCRIPT_SCROLL_STEP, TranscriptMessage, build_transcript_bubbles, layout_transcript_rows,
    transcript_sticky_overlay,
};
use elph_tui::components::theme::UiTheme;
use elph_tui::prelude::*;
use elph_tui::text_editing::transcript_nav_key;

#[derive(Clone, Default, Props)]
pub struct TranscriptPanelProps {
    pub screen_width: u16,
    pub messages: Option<State<Vec<TranscriptMessage>>>,
    pub messages_revision: u64,
    pub sticky_scroll: bool,
    pub keyboard_scroll: bool,
    pub has_focus: bool,
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
    let mut last_sticky_rows = hooks.use_ref(|| 0u16);
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

    let handle = scroll_handle.read();
    let scroll_zone = handle.viewport_height().max(1);
    let panel_viewport = scroll_zone.saturating_add(*last_sticky_rows.read());
    let sticky_idx = props
        .sticky_scroll
        .then(|| {
            active_sticky_user_message_index(
                row_layouts,
                is_sticky_prompt,
                handle.scroll_offset(),
                handle.is_auto_scroll_pinned(),
                panel_viewport,
            )
        })
        .flatten();
    let effective_scroll_offset = if handle.is_auto_scroll_pinned() {
        scroll_view_max_offset(handle.content_height(), scroll_zone)
    } else {
        handle.scroll_offset()
    };
    let suppress_sticky_source =
        sticky_source_bubble_suppressed(row_layouts, sticky_idx, effective_scroll_offset, scroll_zone);
    let bubbles = build_transcript_bubbles(props.screen_width, &messages, suppress_sticky_source);
    let panel_height = panel_viewport;
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
    last_sticky_rows.set(sticky_rows);
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

    let theme = UiTheme::default();
    let pad = theme.shell_zone_padding();
    let scrollbar = theme.scrollbar_style();
    let border_color = if props.has_focus {
        theme.border_focus
    } else {
        theme.border
    };

    element! {
        View(
            width: props.screen_width,
            flex_grow: 1f32,
            flex_shrink: 1f32,
            min_height: 0,
            overflow: Overflow::Hidden,
            border_style: BorderStyle::Single,
            border_edges: Edges::Top,
            border_color: border_color,
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
                        scrollbar_thumb_color: scrollbar.thumb_color,
                        scrollbar_track_color: scrollbar.track_color,
                        keyboard_scroll: Some(props.keyboard_scroll),
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
                            padding_left: pad,
                            padding_right: pad,
                            gap: 0,
                        ) {
                            #(bubbles)
                        }
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
