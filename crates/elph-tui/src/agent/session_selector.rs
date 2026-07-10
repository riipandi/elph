use super::list_modal::render_select_modal;
use crate::bridge::OverlaySlot;
use crate::diff::{OverlayAnchor, OverlayOptions, SelectItem, SelectList, SelectListTheme, SizeValue};
use slt::{Color, Context, KeyCode};

/// Selection state for the session picker overlay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionSelectorState {
    pub selected: usize,
}

/// Outcome of keyboard input while the session selector is visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSelectorAction {
    None,
    Selected(SelectItem),
    Cancelled,
}

/// Handles confirm/cancel keys for the session selector. Call before [`render_session_selector`].
pub fn handle_session_selector_input(
    ui: &Context,
    state: &mut SessionSelectorState,
    sessions: &[SelectItem],
    visible: bool,
) -> SessionSelectorAction {
    if !visible || sessions.is_empty() {
        return SessionSelectorAction::None;
    }

    if ui.raw_key_code(KeyCode::Enter) {
        return sessions
            .get(state.selected)
            .cloned()
            .map(SessionSelectorAction::Selected)
            .unwrap_or(SessionSelectorAction::None);
    }
    if ui.raw_key_code(KeyCode::Esc) {
        return SessionSelectorAction::Cancelled;
    }

    SessionSelectorAction::None
}

/// Renders the session selector as a centered modal list.
pub fn render_session_selector(
    ui: &mut Context,
    sessions: &[SelectItem],
    state: &mut SessionSelectorState,
    visible: bool,
) {
    if !visible || sessions.is_empty() {
        return;
    }

    state.selected = render_select_modal(ui, "Sessions", sessions, state.selected, Color::Blue, 80);
}

/// Builds an overlay slot for session selection (for use with [`OverlayStack`]).
pub fn session_overlay_slot(sessions: Vec<SelectItem>) -> OverlaySlot {
    OverlaySlot::new(
        Box::new(SelectList::new(sessions, 8, SelectListTheme::dark())),
        OverlayOptions {
            width: Some(SizeValue::Percent(80.0)),
            max_height: Some(SizeValue::Percent(60.0)),
            anchor: OverlayAnchor::Center,
            ..Default::default()
        },
    )
}
