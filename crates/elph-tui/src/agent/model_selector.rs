use super::list_modal::render_select_modal;
use crate::bridge::OverlaySlot;
use crate::diff::{OverlayAnchor, OverlayOptions, SelectItem, SelectList, SelectListTheme, SizeValue};
use slt::{Color, Context, KeyCode};

/// Selection state for the model picker overlay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelSelectorState {
    pub selected: usize,
}

/// Outcome of keyboard input while the model selector is visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSelectorAction {
    None,
    Selected(SelectItem),
    Cancelled,
}

/// Handles confirm/cancel keys for the model selector. Call before [`render_model_selector`].
pub fn handle_model_selector_input(
    ui: &Context,
    state: &mut ModelSelectorState,
    models: &[SelectItem],
    visible: bool,
) -> ModelSelectorAction {
    if !visible || models.is_empty() {
        return ModelSelectorAction::None;
    }

    if ui.raw_key_code(KeyCode::Enter) {
        return models
            .get(state.selected)
            .cloned()
            .map(ModelSelectorAction::Selected)
            .unwrap_or(ModelSelectorAction::None);
    }
    if ui.raw_key_code(KeyCode::Esc) {
        return ModelSelectorAction::Cancelled;
    }

    ModelSelectorAction::None
}

/// Renders the model selector as a centered modal list.
pub fn render_model_selector(
    ui: &mut Context,
    models: &[SelectItem],
    current_model: &str,
    state: &mut ModelSelectorState,
    visible: bool,
) {
    if !visible || models.is_empty() {
        return;
    }

    let title = format!("Model: {current_model}");
    state.selected = render_select_modal(ui, &title, models, state.selected, Color::Cyan, 70);
}

/// Builds an overlay slot for model selection.
pub fn model_overlay_slot(models: Vec<SelectItem>) -> OverlaySlot {
    OverlaySlot::new(
        Box::new(SelectList::new(models, 10, SelectListTheme::dark())),
        OverlayOptions {
            width: Some(SizeValue::Percent(70.0)),
            max_height: Some(SizeValue::Percent(50.0)),
            anchor: OverlayAnchor::Center,
            ..Default::default()
        },
    )
}
