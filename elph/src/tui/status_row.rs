//! Status row between transcript and editor.

use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct StatusRowProps {
    pub screen_width: u16,
    pub time_label: String,
}

#[component]
pub fn StatusRow(props: &StatusRowProps) -> impl Into<AnyElement<'static>> {
    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
        ) {
            View(
                width: props.screen_width / 2,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Start,
                padding: 0,
            ) {
                Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: props.time_label.clone())
            }
            View(
                width: props.screen_width / 2,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::End,
                padding: 0,
            ) {
                Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: "Enter send · Shift+Enter/Ctrl+J newline · Shift+↑↓ scroll · Ctrl+D quit")
            }
        }
    }
}
