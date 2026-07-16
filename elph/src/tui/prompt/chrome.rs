//! Editor + footer column (bottom chrome).

use iocraft::prelude::*;

use crate::types::{AgentMode, ThinkingLevel};

use super::editor::Editor;
use super::footer::Footer;
use crate::tui::slash_palette::{SlashCommandPalette, SlashPaletteSnapshot, palette_anchor_bottom};

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
    pub force_editor_clear: Option<Ref<bool>>,
    pub slash_palette_snapshot: SlashPaletteSnapshot,
    pub slash_palette_selected: Option<State<usize>>,
    pub on_submit: HandlerMut<'static, String>,
}

#[component]
pub fn PromptChrome(props: &mut PromptChromeProps) -> impl Into<AnyElement<'static>> {
    let draft_text = props
        .live_draft
        .as_ref()
        .map(|live| live.read().clone())
        .or_else(|| props.draft.as_ref().map(|draft| draft.read().clone()))
        .unwrap_or_default();
    let palette_anchor = palette_anchor_bottom(&draft_text, props.screen_width, props.screen_height);

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
            View(
                width: props.screen_width,
                flex_shrink: 0f32,
                position: Position::Relative,
                align_items: AlignItems::FlexStart,
            ) {
                Editor(
                    screen_width: props.screen_width,
                    screen_height: props.screen_height,
                    agent_mode: props.agent_mode,
                    draft: props.draft,
                    live_draft: props.live_draft,
                    suppress_enter_newline: props.suppress_enter_newline,
                    force_clear: props.force_editor_clear,
                    on_submit: props.on_submit.take(),
                )
                SlashCommandPalette(
                    screen_width: props.screen_width,
                    screen_height: props.screen_height,
                    agent_mode: props.agent_mode,
                    snapshot: props.slash_palette_snapshot.clone(),
                    anchor_bottom: palette_anchor,
                    selected_index: props.slash_palette_selected,
                )
            }
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
