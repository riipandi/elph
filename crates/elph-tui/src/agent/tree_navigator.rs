use super::list_modal::render_select_modal;
use crate::bridge::OverlaySlot;
use crate::diff::{OverlayAnchor, OverlayOptions, SelectItem, SelectList, SelectListTheme, SizeValue};
use slt::{Color, Context, KeyCode};

/// Selection state for the session tree navigator overlay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeNavigatorState {
    pub selected: usize,
}

/// Outcome of keyboard input while the tree navigator is visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeNavigatorAction {
    None,
    Selected(SelectItem),
    Cancelled,
}

/// Handles confirm/cancel keys for the tree navigator. Call before [`render_tree_navigator`].
pub fn handle_tree_navigator_input(
    ui: &Context,
    state: &mut TreeNavigatorState,
    entries: &[SelectItem],
    visible: bool,
) -> TreeNavigatorAction {
    if !visible || entries.is_empty() {
        return TreeNavigatorAction::None;
    }

    if ui.raw_key_code(KeyCode::Enter) {
        return entries
            .get(state.selected)
            .cloned()
            .map(TreeNavigatorAction::Selected)
            .unwrap_or(TreeNavigatorAction::None);
    }
    if ui.raw_key_code(KeyCode::Esc) {
        return TreeNavigatorAction::Cancelled;
    }

    TreeNavigatorAction::None
}

/// Renders the tree navigator as a centered modal list.
pub fn render_tree_navigator(ui: &mut Context, entries: &[SelectItem], state: &mut TreeNavigatorState, visible: bool) {
    if !visible || entries.is_empty() {
        return;
    }

    state.selected = render_select_modal(ui, "Session tree", entries, state.selected, Color::Magenta, 85);
}

/// Builds an overlay slot for tree navigation.
pub fn tree_overlay_slot(entries: Vec<SelectItem>) -> OverlaySlot {
    OverlaySlot::new(
        Box::new(SelectList::new(entries, 12, SelectListTheme::dark())),
        OverlayOptions {
            width: Some(SizeValue::Percent(85.0)),
            max_height: Some(SizeValue::Percent(65.0)),
            anchor: OverlayAnchor::Center,
            ..Default::default()
        },
    )
}
