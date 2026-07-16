//! Slim bordered palette: title chip + command list.

use iocraft::prelude::*;

use super::super::model::SlashPaletteSnapshot;
use super::body::PaletteCardBody;
use super::chrome::PaletteCardChrome;
use super::frame::PaletteCardFrame;
use crate::types::AgentMode;

#[derive(Clone, Default, Props)]
pub struct SlashPaletteCardProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub agent_mode: AgentMode,
    pub snapshot: SlashPaletteSnapshot,
    pub selected_index: Option<State<usize>>,
}

#[component]
pub fn SlashPaletteCard(props: &SlashPaletteCardProps) -> impl Into<AnyElement<'static>> {
    let chrome = PaletteCardChrome::from_snapshot(props.screen_width, props.agent_mode, &props.snapshot);

    element! {
        PaletteCardFrame(chrome: chrome.clone()) {
            PaletteCardBody(
                chrome: chrome,
                snapshot: props.snapshot.clone(),
                selected_index: props.selected_index,
                screen_height: props.screen_height,
            )
        }
    }
}
