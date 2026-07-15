//! Editor + footer column (bottom chrome).

use iocraft::prelude::*;

use crate::types::{AgentMode, ThinkingLevel};

use super::editor::Editor;
use super::footer::Footer;

#[derive(Default, Props)]
pub struct PromptChromeProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub agent_mode: AgentMode,
    pub thinking_level: ThinkingLevel,
    pub project_label: String,
    pub model_label: String,
    pub supports_images: bool,
    pub draft: Option<State<String>>,
    pub live_draft: Option<Ref<String>>,
    pub suppress_enter_newline: Option<Ref<bool>>,
    pub on_submit: HandlerMut<'static, String>,
}

#[component]
pub fn PromptChrome(props: &mut PromptChromeProps) -> impl Into<AnyElement<'static>> {
    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            border_style: BorderStyle::None,
            align_items: AlignItems::FlexStart,
            flex_direction: FlexDirection::Column,
            margin_bottom: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: 0,
            padding_right: 0,
        ) {
            Editor(
                screen_width: props.screen_width,
                screen_height: props.screen_height,
                agent_mode: props.agent_mode,
                draft: props.draft,
                live_draft: props.live_draft,
                suppress_enter_newline: props.suppress_enter_newline,
                on_submit: props.on_submit.take(),
            )
            Footer(
                screen_width: props.screen_width,
                project_label: props.project_label.clone(),
                model_label: props.model_label.clone(),
                thinking_level: props.thinking_level,
                supports_images: props.supports_images,
            )
        }
    }
}
