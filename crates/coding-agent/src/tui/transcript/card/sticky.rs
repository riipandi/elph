//! Sticky user-prompt overlay pinned above the scroll viewport.

use iocraft::prelude::*;

use crate::tui::theme::{TEXT_FG, USER_INPUT_ACCENT, USER_INPUT_BG};

use super::super::types::TranscriptMessage;
use super::timestamp_layout::{render_sticky_prompt_row, user_input_right_rail};

pub fn transcript_sticky_overlay(
    height: u16,
    inner_width: u16,
    message: &TranscriptMessage,
    display_content: &str,
) -> AnyElement<'static> {
    let style = message.style;
    let pad_h = style.horizontal_padding();
    let right_rail = user_input_right_rail(message.submitted_at, None);
    let body = render_sticky_prompt_row(inner_width, display_content, right_rail.as_deref(), TEXT_FG);
    element! {
        View(
            position: Position::Absolute,
            top: 0,
            left: 0,
            right: 1,
            height: height,
            overflow: Overflow::Hidden,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            padding_left: 1,
            padding_right: 1,
            padding_bottom: 1,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Baseline,
        ) {
            View(
                width: 100pct,
                background_color: USER_INPUT_BG,
                border_style: BorderStyle::Bold,
                border_edges: Edges::Left,
                border_color: USER_INPUT_ACCENT,
                padding_top: style.sticky_padding_top(),
                padding_bottom: style.sticky_padding_bottom(),
                padding_left: pad_h,
                padding_right: pad_h,
                flex_shrink: 0f32,
                margin_bottom: 0,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                gap: 0,
            ) {
                #(body)
            }
        }
    }
    .into()
}
