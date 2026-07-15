//! Session header bar (top chrome).

use iocraft::prelude::*;

use super::theme::BORDER_MUTED;

#[derive(Default, Props)]
pub struct HeaderProps {
    pub screen_width: u16,
    pub session_label: String,
    pub stats_label: String,
}

#[component]
pub fn Header(props: &HeaderProps) -> impl Into<AnyElement<'static>> {
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
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: props.session_label.clone())
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: props.stats_label.clone())
        }
    }
}
