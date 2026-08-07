//! Floating shell that anchors the palette card above the editor.

use iocraft::prelude::*;

use super::card::SlashPaletteCard;
use super::model::SlashPaletteSnapshot;
use crate::types::AgentMode;

#[derive(Clone, Default, Props)]
pub struct SlashCommandPaletteProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub agent_mode: AgentMode,
    pub snapshot: SlashPaletteSnapshot,
    pub anchor_bottom: u16,
    pub selected_index: Option<State<usize>>,
}

#[component]
pub fn SlashCommandPalette(props: &SlashCommandPaletteProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    if !props.snapshot.should_render() {
        return element! { View(width: 0u16, height: 0u16) {} };
    }

    element! {
        View(
            width: props.screen_width,
            position: Position::Absolute,
            left: 0,
            bottom: props.anchor_bottom,
            flex_shrink: 0f32,
            align_items: AlignItems::FlexStart,
        ) {
            SlashPaletteCard(
                screen_width: props.screen_width,
                screen_height: props.screen_height,
                agent_mode: props.agent_mode,
                snapshot: props.snapshot.clone(),
                selected_index: props.selected_index,
            )
        }
    }
}
