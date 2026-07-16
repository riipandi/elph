//! Session header bar (top chrome).

use iocraft::prelude::*;

use super::fit::{chrome_half_width, fit_header_stats, fit_session_header_left};
use crate::tui::theme::BORDER_MUTED;

#[derive(Default, Props)]
pub struct HeaderProps {
    pub screen_width: u16,
    pub session_id: String,
    pub mcp_connected: usize,
    pub skills_count: usize,
    pub cost_usd: f64,
    pub tokens_used: u64,
    pub context_pct: f64,
    pub context_limit: u64,
    pub token_display: String,
}

#[component]
pub fn Header(props: &HeaderProps) -> impl Into<AnyElement<'static>> {
    let half = chrome_half_width(props.screen_width);
    let session_label = fit_session_header_left(&props.session_id, props.mcp_connected, props.skills_count, half);
    let stats_label = fit_header_stats(
        props.cost_usd,
        props.tokens_used,
        props.context_pct,
        props.context_limit,
        &props.token_display,
        half,
    );

    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            background_color: Color::Reset,
            border_style: BorderStyle::Single,
            border_edges: Edges::Top,
            border_color: BORDER_MUTED,
            position: Position::Relative,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
            margin_bottom: 0,
        ) {
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: session_label)
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: stats_label)
        }
    }
}
