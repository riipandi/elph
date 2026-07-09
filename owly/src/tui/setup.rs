//! In-TUI first-run credential setup wizard.

use elph_tui::{LineComponent, SelectItem, SelectList, SelectListTheme, Theme};
use iocraft::prelude::*;

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

#[derive(Default, Props)]
pub struct SetupWizardProps {
    pub default_provider: String,
    pub default_model: String,
    pub theme: Theme,
    pub setup_error: Option<State<Option<String>>>,
    pub on_complete: HandlerMut<'static, SetupCredentials>,
}

#[component]
pub fn SetupWizard(mut hooks: Hooks, props: &mut SetupWizardProps) -> impl Into<AnyElement<'static>> {
    let providers: Vec<SelectItem> = provider_select_items()
        .into_iter()
        .map(|(value, label)| SelectItem::new(value, label))
        .collect();

    let default_provider_idx = providers
        .iter()
        .position(|item| item.value == props.default_provider)
        .unwrap_or(0);

    let mut step = hooks.use_state(|| SetupStep::Provider);
    let mut selected_provider_idx = hooks.use_state(move || default_provider_idx);
    let mut api_key = hooks.use_state(String::new);
    let mut base_url = hooks.use_state(String::new);
    let mut model_id = hooks.use_state(|| props.default_model.clone());
    let mut text_input = hooks.use_state(String::new);
    let mut on_complete = props.on_complete.take();
    let theme = props.theme;
    let setup_error = props.setup_error;
    let (term_width, term_height) = hooks.use_terminal_size();

    let advance_from_provider = {
        let providers = providers.clone();
        move |selected_idx: usize, model: &mut State<String>, input: &mut State<String>| {
            if let Some(item) = providers.get(selected_idx) {
                if let Some(default_model) = default_model_for_provider(&item.value) {
                    model.set(default_model);
                }
                input.set(String::new());
            }
        }
    };

    hooks.use_terminal_events({
        let providers = providers.clone();
        move |event| {
            let TerminalEvent::Key(KeyEvent {
                code, kind, modifiers, ..
            }) = event
            else {
                return;
            };
            if kind == KeyEventKind::Release {
                return;
            }

            match step.get() {
                SetupStep::Provider => match code {
                    KeyCode::Up => {
                        let next = if selected_provider_idx.get() == 0 {
                            providers.len().saturating_sub(1)
                        } else {
                            selected_provider_idx.get() - 1
                        };
                        selected_provider_idx.set(next);
                    }
                    KeyCode::Down => {
                        let len = providers.len();
                        if len == 0 {
                            return;
                        }
                        let next = if selected_provider_idx.get() + 1 >= len {
                            0
                        } else {
                            selected_provider_idx.get() + 1
                        };
                        selected_provider_idx.set(next);
                    }
                    KeyCode::Enter => {
                        advance_from_provider(selected_provider_idx.get(), &mut model_id, &mut text_input);
                        step.set(SetupStep::ApiKey);
                    }
                    _ => {}
                },
                SetupStep::ApiKey | SetupStep::BaseUrl | SetupStep::Model => {
                    if code == KeyCode::Esc {
                        match step.get() {
                            SetupStep::ApiKey => step.set(SetupStep::Provider),
                            SetupStep::BaseUrl => step.set(SetupStep::ApiKey),
                            SetupStep::Model => {
                                let provider = providers
                                    .get(selected_provider_idx.get())
                                    .map(|item| item.value.as_str())
                                    .unwrap_or("");
                                if setup_collects_base_url(provider) {
                                    step.set(SetupStep::BaseUrl);
                                } else {
                                    step.set(SetupStep::ApiKey);
                                }
                            }
                            SetupStep::Provider => {}
                        }
                        text_input.set(String::new());
                        return;
                    }

                    if is_submit_key(code, modifiers) {
                        let value = text_input.read().clone().trim().to_string();
                        match step.get() {
                            SetupStep::ApiKey => {
                                if value.is_empty() {
                                    return;
                                }
                                api_key.set(value);
                                text_input.set(String::new());
                                let provider = providers
                                    .get(selected_provider_idx.get())
                                    .map(|item| item.value.as_str())
                                    .unwrap_or("");
                                if setup_collects_base_url(provider) {
                                    step.set(SetupStep::BaseUrl);
                                } else {
                                    step.set(SetupStep::Model);
                                }
                            }
                            SetupStep::BaseUrl => {
                                let provider = providers
                                    .get(selected_provider_idx.get())
                                    .map(|item| item.value.as_str())
                                    .unwrap_or("");
                                if setup_base_url_required(provider) && value.is_empty() {
                                    if let Some(mut error) = setup_error {
                                        error.set(Some(format!(
                                            "{} is required.",
                                            base_url_label(provider).unwrap_or_else(|| "Base URL".to_string())
                                        )));
                                    }
                                    return;
                                }
                                base_url.set(value);
                                text_input.set(String::new());
                                step.set(SetupStep::Model);
                            }
                            SetupStep::Model => {
                                let model = if value.is_empty() {
                                    model_id.read().clone()
                                } else {
                                    value
                                };
                                model_id.set(model.clone());
                                let provider = providers
                                    .get(selected_provider_idx.get())
                                    .map(|item| item.value.clone())
                                    .unwrap_or_default();
                                let base = {
                                    let trimmed = base_url.read().clone().trim().to_string();
                                    if trimmed.is_empty() { None } else { Some(trimmed) }
                                };
                                on_complete(SetupCredentials {
                                    provider,
                                    api_key: api_key.read().clone(),
                                    base_url: base,
                                    model_id: model,
                                });
                            }
                            SetupStep::Provider => {}
                        }
                        return;
                    }

                    handle_text_key(&mut text_input, code);
                }
            }
        }
    });

    let error_line = setup_error
        .as_ref()
        .and_then(|state| state.read().clone())
        .unwrap_or_default();

    let body_lines = match step.get() {
        SetupStep::Provider => {
            let mut list = SelectList::new(providers.clone(), 8, SelectListTheme::dark());
            list.set_selected_index(selected_provider_idx.get());
            let mut rendered = list.render(term_width.saturating_sub(8).max(40));
            rendered.insert(0, ">_ Owly setup".to_string());
            rendered.insert(1, "Configure your inference provider and API key.".to_string());
            rendered.insert(2, String::new());
            rendered.insert(3, "Select provider (↑/↓, Enter):".to_string());
            rendered
        }
        SetupStep::ApiKey => {
            let provider = providers
                .get(selected_provider_idx.get())
                .map(|item| item.value.as_str())
                .unwrap_or("");
            let prompt = api_key_label(provider).unwrap_or_else(|| "API key".to_string());
            vec![
                ">_ Owly setup".to_string(),
                String::new(),
                prompt,
                format!("> {}", mask_secret(&text_input.read().clone())),
                String::new(),
                "Enter to continue · Esc to go back".to_string(),
            ]
        }
        SetupStep::BaseUrl => {
            let provider = providers
                .get(selected_provider_idx.get())
                .map(|item| item.value.as_str())
                .unwrap_or("");
            let prompt = base_url_label(provider).unwrap_or_else(|| "Base URL".to_string());
            vec![
                ">_ Owly setup".to_string(),
                String::new(),
                prompt,
                format!("> {}", text_input.read().clone()),
                String::new(),
                "Enter to continue · Esc to go back".to_string(),
            ]
        }
        SetupStep::Model => {
            vec![
                ">_ Owly setup".to_string(),
                String::new(),
                "Model ID".to_string(),
                format!(
                    "> {}",
                    if text_input.read().clone().is_empty() {
                        model_id.read().clone()
                    } else {
                        text_input.read().clone()
                    }
                ),
                String::new(),
                "Enter to save · Esc to go back".to_string(),
            ]
        }
    };

    if !error_line.is_empty() {
        let mut with_error = body_lines;
        with_error.push(String::new());
        with_error.push(format!("Error: {error_line}"));
        render_setup_screen(term_width, term_height, theme, with_error)
    } else {
        render_setup_screen(term_width, term_height, theme, body_lines)
    }
}

fn render_setup_screen(width: u16, height: u16, theme: Theme, lines: Vec<String>) -> impl Into<AnyElement<'static>> {
    element! {
        View(
            width,
            height,
            background_color: theme.view_background(),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        ) {
            View(
                border_style: BorderStyle::Round,
                border_color: subtle_border(theme),
                padding: 2,
                width: 72pct,
                flex_direction: FlexDirection::Column,
                gap: Gap::Length(1),
            ) {
                Text(content: lines.join("\n"))
            }
        }
    }
}

fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "•".repeat(value.chars().count())
    }
}

fn is_submit_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::Enter && !modifiers.contains(KeyModifiers::SHIFT)
}

fn handle_text_key(input: &mut State<String>, code: KeyCode) {
    match code {
        KeyCode::Char(ch) => {
            let mut next = input.read().clone();
            next.push(ch);
            input.set(next);
        }
        KeyCode::Backspace => {
            let mut next = input.read().clone();
            next.pop();
            input.set(next);
        }
        _ => {}
    }
}
