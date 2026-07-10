use super::list_modal::render_select_modal;
use crate::diff::SelectItem;
use slt::{Color, Context, KeyCode};

/// Selection state for the OAuth provider picker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OAuthSelectorState {
    pub selected: usize,
}

/// Outcome of keyboard input while the OAuth selector is visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthSelectorAction {
    None,
    Selected(SelectItem),
    Cancelled,
}

/// Mock OAuth provider list for TUI-only integration.
pub fn mock_oauth_providers() -> Vec<SelectItem> {
    vec![
        SelectItem::new("anthropic", "Anthropic").with_description("Claude models"),
        SelectItem::new("openai", "OpenAI").with_description("GPT models"),
        SelectItem::new("google", "Google").with_description("Gemini models"),
    ]
}

/// Handles confirm/cancel keys for the OAuth selector. Call before [`render_oauth_selector`].
pub fn handle_oauth_selector_input(
    ui: &Context,
    state: &mut OAuthSelectorState,
    providers: &[SelectItem],
    visible: bool,
) -> OAuthSelectorAction {
    if !visible || providers.is_empty() {
        return OAuthSelectorAction::None;
    }

    if ui.raw_key_code(KeyCode::Enter) {
        return providers
            .get(state.selected)
            .cloned()
            .map(OAuthSelectorAction::Selected)
            .unwrap_or(OAuthSelectorAction::None);
    }
    if ui.raw_key_code(KeyCode::Esc) {
        return OAuthSelectorAction::Cancelled;
    }

    OAuthSelectorAction::None
}

/// Renders the OAuth provider selector as a centered modal list.
pub fn render_oauth_selector(
    ui: &mut Context,
    providers: &[SelectItem],
    state: &mut OAuthSelectorState,
    visible: bool,
) {
    if !visible || providers.is_empty() {
        return;
    }

    state.selected = render_select_modal(ui, "Select provider:", providers, state.selected, Color::Yellow, 60);
}
