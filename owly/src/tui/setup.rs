//! In-TUI first-run credential setup wizard.

use elph_tui::Theme;
use slt::{Border, Context, KeyCode, ListState, TextInputState};

use crate::onboarding::{
    SetupCredentials, api_key_label, base_url_label, default_model_for_provider, provider_select_items,
    setup_base_url_required, setup_collects_base_url,
};

use super::chrome::subtle_border;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupStep {
    Provider,
    ApiKey,
    BaseUrl,
    Model,
}

pub struct SetupWizardState {
    step: SetupStep,
    providers: Vec<String>,
    list: ListState,
    api_key: String,
    base_url: String,
    model_id: String,
    draft: String,
    error: Option<String>,
}

impl SetupWizardState {
    pub fn new(default_provider: &str, default_model: &str) -> Self {
        let items: Vec<(String, String)> = provider_select_items();
        let provider_labels: Vec<String> = items.iter().map(|(_, label)| label.clone()).collect();
        let providers: Vec<String> = items.into_iter().map(|(value, _)| value).collect();
        let selected = providers.iter().position(|p| p == default_provider).unwrap_or(0);

        let mut list = ListState::new(provider_labels);
        list.selected = selected;

        Self {
            step: SetupStep::Provider,
            providers,
            list,
            api_key: String::new(),
            base_url: String::new(),
            model_id: default_model.to_string(),
            draft: String::new(),
            error: None,
        }
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn handle_keys(&mut self, ui: &mut Context) -> Option<SetupCredentials> {
        if ui.key_code(KeyCode::Esc) {
            match self.step {
                SetupStep::Provider => {}
                SetupStep::ApiKey => self.step = SetupStep::Provider,
                SetupStep::BaseUrl => self.step = SetupStep::ApiKey,
                SetupStep::Model => {
                    let provider = self.current_provider();
                    self.step = if setup_collects_base_url(provider) {
                        SetupStep::BaseUrl
                    } else {
                        SetupStep::ApiKey
                    };
                }
            }
            self.draft.clear();
            return None;
        }

        match self.step {
            SetupStep::Provider => {
                if ui.key_code(KeyCode::Enter) {
                    if let Some(provider) = self.providers.get(self.list.selected)
                        && let Some(default_model) = default_model_for_provider(provider)
                    {
                        self.model_id = default_model.to_string();
                    }
                    self.draft.clear();
                    self.step = SetupStep::ApiKey;
                }
            }
            SetupStep::ApiKey | SetupStep::BaseUrl | SetupStep::Model => {
                if ui.key_code(KeyCode::Enter) {
                    match self.step {
                        SetupStep::ApiKey => {
                            self.api_key = self.draft.clone();
                            self.draft.clear();
                            let provider = self.current_provider();
                            self.step = if setup_collects_base_url(provider) {
                                SetupStep::BaseUrl
                            } else {
                                SetupStep::Model
                            };
                        }
                        SetupStep::BaseUrl => {
                            self.base_url = self.draft.clone();
                            self.draft.clear();
                            self.step = SetupStep::Model;
                        }
                        SetupStep::Model => {
                            if !self.draft.is_empty() {
                                self.model_id = self.draft.clone();
                            }
                            return Some(self.build_credentials());
                        }
                        SetupStep::Provider => {}
                    }
                }
            }
        }
        None
    }

    fn current_provider(&self) -> &str {
        self.providers.get(self.list.selected).map(String::as_str).unwrap_or("")
    }

    fn build_credentials(&self) -> SetupCredentials {
        let provider = self.current_provider().to_string();
        SetupCredentials {
            provider,
            api_key: self.api_key.clone(),
            base_url: if setup_base_url_required(self.current_provider()) {
                Some(self.base_url.clone())
            } else {
                None
            },
            model_id: self.model_id.clone(),
        }
    }

    fn sync_input(&mut self, input: &TextInputState) {
        self.draft = input.value.clone();
    }

    fn fill_input(&self, input: &mut TextInputState) {
        input.value = self.draft.clone();
        input.cursor = input.value.chars().count();
    }
}

pub fn render_setup_wizard(ui: &mut Context, state: &mut SetupWizardState, theme: Theme) {
    let mut text_input = TextInputState::new();
    state.fill_input(&mut text_input);

    let _ = ui
        .bordered(Border::Rounded)
        .border_fg(subtle_border(theme))
        .p(2)
        .gap(1)
        .col(|ui| {
            let _ = ui.text(">_ Owly setup").bold();
            let _ = ui.text("Configure your inference provider and API key.").dim();

            match state.step {
                SetupStep::Provider => {
                    let _ = ui.text("Select provider (↑/↓, Enter):").dim();
                    let _ = ui.list(&mut state.list);
                }
                SetupStep::ApiKey => {
                    let prompt = api_key_label(state.current_provider()).unwrap_or_else(|| "API key".into());
                    let _ = ui.text(prompt);
                    let _ = ui.text_input(&mut text_input);
                    let _ = ui.text("Enter to continue · Esc to go back").dim();
                }
                SetupStep::BaseUrl => {
                    let prompt = base_url_label(state.current_provider()).unwrap_or_else(|| "Base URL".into());
                    let _ = ui.text(prompt);
                    let _ = ui.text_input(&mut text_input);
                    let _ = ui.text("Enter to continue · Esc to go back").dim();
                }
                SetupStep::Model => {
                    let _ = ui.text("Model ID");
                    if text_input.value.is_empty() {
                        text_input.value = state.model_id.clone();
                    }
                    let _ = ui.text_input(&mut text_input);
                    let _ = ui.text("Enter to continue · Esc to go back").dim();
                }
            }

            if let Some(err) = &state.error {
                let _ = ui.text(format!("Error: {err}")).fg(slt::Color::Red);
            }
        });

    if state.step != SetupStep::Provider {
        state.sync_input(&text_input);
    }
}
