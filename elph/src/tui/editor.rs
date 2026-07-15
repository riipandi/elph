//! Multiline prompt editor with agent-mode overlap label.

use elph_tui::Textarea;
use iocraft::prelude::*;

use crate::types::AgentMode;

use super::theme::{EDITOR_BORDER, rgb_color};

fn editor_max_height(screen_height: u16) -> u16 {
    (screen_height / 4).clamp(4, 12)
}

#[derive(Default, Props)]
pub struct EditorProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub agent_mode: AgentMode,
    pub draft: Option<State<String>>,
    pub live_draft: Option<Ref<String>>,
    pub suppress_enter_newline: Option<Ref<bool>>,
    pub on_submit: HandlerMut<'static, String>,
}

#[component]
pub fn Editor(props: &mut EditorProps) -> impl Into<AnyElement<'static>> {
    let label_color = rgb_color(props.agent_mode.label_rgb());

    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            border_style: BorderStyle::Round,
            border_color: EDITOR_BORDER,
            position: Position::Relative,
            align_items: AlignItems::FlexStart,
            margin_bottom: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: 1,
            padding_right: 1,
            overflow: Overflow::Hidden,
        ) {
            Textarea(
                width: props.screen_width.saturating_sub(2),
                min_height: 1u16,
                max_height: Some(editor_max_height(props.screen_height)),
                show_border: Some(false),
                has_focus: true,
                value: props.draft,
                live_draft: props.live_draft,
                suppress_enter_newline: props.suppress_enter_newline,
                submit_on_enter: true,
                on_submit: props.on_submit.take(),
                text_color: Some(Color::Grey),
                cursor_color: Some(Color::DarkGrey),
            )
            View(
                position: Position::Absolute,
                right: 1,
                bottom: 0,
                margin_bottom: -1,
                background_color: Color::Reset,
            ) {
                Text(
                    color: label_color,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                    content: format!(" {} ", props.agent_mode.footer_label()),
                )
            }
        }
    }
}
