//! `/provider connect` dialog — OAuth provider selection and API key input.

use elph_ai::{get_builtin_providers, builtin_oauth_provider_ids};
use elph_tui::components::{DialogUserInputContent, SelectList, UiTheme};
use elph_tui::types::SelectOption;
use iocraft::prelude::*;

use crate::tui::focus::ShellFocus;
use crate::tui::inline_dialog::{InlineDialogShell, inline_body_width};

/// Provider information for the selection list.
#[derive(Debug, Clone)]
pub struct ProviderOption {
    pub id: String,
    pub name: String,
    pub supports_oauth: bool,
}

/// Get list of all providers with OAuth support info.
pub fn get_provider_options() -> Vec<ProviderOption> {
    let oauth_provider_ids = builtin_oauth_provider_ids();
    
    get_builtin_providers()
        .into_iter()
        .map(|id| ProviderOption {
            id: id.to_string(),
            name: format_provider_name(id),
            supports_oauth: oauth_provider_ids.contains(&id),
        })
        .collect()
}

/// Format provider name for display.
pub fn format_provider_name(id: &str) -> String {
    // Map provider IDs to display names
    match id {
        "anthropic" => "Anthropic".to_string(),
        "openai" => "OpenAI".to_string(),
        "openai-codex" => "OpenAI Codex".to_string(),
        "github-copilot" => "GitHub Copilot".to_string(),
        "hyper" => "Hyper".to_string(),
        "xai" => "xAI".to_string(),
        "google" => "Google".to_string(),
        "google-vertex" => "Google Vertex AI".to_string(),
        "amazon-bedrock" => "Amazon Bedrock".to_string(),
        "cloudflare-ai-gateway" => "Cloudflare AI Gateway".to_string(),
        "cloudflare-workers-ai" => "Cloudflare Workers AI".to_string(),
        "fireworks" => "Fireworks".to_string(),
        "groq" => "Groq".to_string(),
        "deepseek" => "DeepSeek".to_string(),
        "huggingface" => "Hugging Face".to_string(),
        "kimi-coding" => "Kimi Coding".to_string(),
        "xiaomi" => "Xiaomi".to_string(),
        "zai" => "ZAI".to_string(),
        "cerebras" => "Cerebras".to_string(),
        "kilo" => "Kilo Gateway".to_string(),
        "faux" => "Faux".to_string(),
        _ => id.replace('-', " ").replace('_', " "),
    }
}

/// Footer hint for the provider connect dialog.
pub fn provider_connect_footer_hint() -> String {
    "↑↓ move · Enter select · Esc cancel".to_string()
}

/// Pending provider connection dialog state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingProviderConnectDialog {
    /// Selected provider index in the list.
    pub selected_provider: Option<usize>,
    /// When a non-OAuth provider is selected, this holds the API key input.
    pub api_key_input: Option<String>,
    /// The provider ID being connected (if directly specified).
    pub provider_id: Option<String>,
    /// Prompt draft stashed while the dialog is open.
    pub stashed_prompt_draft: Option<String>,
}

/// Arguments for [`open_provider_connect_dialog`].
pub struct OpenProviderConnectDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingProviderConnectDialog>>,
    pub selected: &'a mut State<usize>,
    pub api_key_input: &'a mut State<String>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub provider_id: Option<String>,
}

/// Open the provider connect dialog.
pub fn open_provider_connect_dialog(args: OpenProviderConnectDialogArgs<'_>) {
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }
    
    let provider_options = get_provider_options();
    let initial_selected = args.provider_id.as_ref()
        .and_then(|id| provider_options.iter().position(|p| p.id == *id))
        .unwrap_or(0);
    
    args.selected.set(initial_selected);
    args.api_key_input.set(String::new());
    
    args.pending.set(Some(PendingProviderConnectDialog {
        selected_provider: Some(initial_selected),
        api_key_input: None,
        provider_id: args.provider_id,
        stashed_prompt_draft: stashed,
    }));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

/// Close the provider connect dialog.
pub fn close_provider_connect_dialog(
    pending: &mut Ref<Option<PendingProviderConnectDialog>>,
    selected: &mut State<usize>,
    api_key_input: &mut State<String>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
    restore_stash: bool,
) {
    let stashed = pending.write().take().and_then(|p| p.stashed_prompt_draft);
    selected.set(0);
    api_key_input.set(String::new());
    
    if restore_stash {
        if let Some(text) = stashed {
            draft.set(text.clone());
            live_draft.set(text);
        } else {
            draft.set(String::new());
            live_draft.set(String::new());
        }
    } else {
        draft.set(String::new());
        live_draft.set(String::new());
    }
    shell_focus.set(ShellFocus::Prompt);
}

/// Get the provider at a given index.
pub fn get_provider_at_index(index: usize) -> Option<ProviderOption> {
    get_provider_options().get(index).cloned()
}

/// Check if provider supports OAuth.
pub fn provider_supports_oauth(provider_id: &str) -> bool {
    builtin_oauth_provider_ids().contains(&provider_id)
}

/// Render the provider connect dialog.
pub fn render_provider_connect_dialog(
    screen_width: u16,
    has_focus: bool,
    selected: State<usize>,
    api_key_input: State<String>,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);
    
    let provider_options = get_provider_options();
    let options: Vec<SelectOption> = provider_options
        .iter()
        .map(|p| {
            let suffix = if p.supports_oauth { 
                " (OAuth)".to_string() 
            } else {
                " (API Key)".to_string()
            };
            SelectOption::new(&format!("{}{}", p.name, suffix), &p.id)
        })
        .collect();
    
    let selected_idx = selected.get();
    let selected_provider = provider_options.get(selected_idx);
    let show_api_key_input = selected_provider.map_or(false, |p| !p.supports_oauth);
    let api_key_label = selected_provider.map(|p| format!("Enter {} API Key:", p.name)).unwrap_or_default();
    
    let w = body_width;
    let hf = has_focus;
    let thm = theme;
    
    if show_api_key_input {
        element! {
            InlineDialogShell(
                screen_width: screen_width,
                title: "Connect Provider".to_string(),
                has_focus: has_focus,
                footer_hint: Some(provider_connect_footer_hint()),
            ) {
                View(
                    width: w,
                    flex_direction: FlexDirection::Column,
                    flex_shrink: 0f32,
                ) {
                    View(
                        width: w,
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0f32,
                    ) {
                        View(
                            width: w,
                            flex_shrink: 0f32,
                            overflow: Overflow::Hidden,
                        ) {
                            Text(
                                content: "Select a provider to connect:".to_string(),
                                color: thm.text_secondary,
                                wrap: TextWrap::Wrap,
                            )
                        }
                        View(
                            width: w,
                            padding_top: 1,
                            flex_shrink: 0f32,
                        ) {
                            SelectList(
                                width: w,
                                height: 8u16,
                                options: options,
                                selected_index: Some(selected.clone()),
                                has_focus: hf,
                                show_description: false,
                                compact: true,
                                theme: Some(thm),
                            )
                        }
                    }
                    View(
                        width: w,
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0f32,
                    ) {
                        View(
                            width: w,
                            padding_top: 1,
                            flex_shrink: 0f32,
                        ) {
                            Text(
                                content: api_key_label,
                                color: thm.text_secondary,
                                wrap: TextWrap::Wrap,
                            )
                        }
                        View(
                            width: w,
                            padding_top: 1,
                            flex_shrink: 0f32,
                        ) {
                            DialogUserInputContent(
                                width: w,
                                value: Some(api_key_input.clone()),
                                has_focus: hf,
                                theme: Some(thm),
                                compact: true,
                                show_prompt: false,
                                show_footer_hint: false,
                                dialog_chrome: true,
                                on_submit: HandlerMut::default(),
                                on_cancel: HandlerMut::default(),
                            )
                        }
                    }
                }
            }
        }
        .into()
    } else {
        element! {
            InlineDialogShell(
                screen_width: screen_width,
                title: "Connect Provider".to_string(),
                has_focus: has_focus,
                footer_hint: Some(provider_connect_footer_hint()),
            ) {
                View(
                    width: w,
                    flex_direction: FlexDirection::Column,
                    flex_shrink: 0f32,
                ) {
                    View(
                        width: w,
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0f32,
                    ) {
                        View(
                            width: w,
                            flex_shrink: 0f32,
                            overflow: Overflow::Hidden,
                        ) {
                            Text(
                                content: "Select a provider to connect:".to_string(),
                                color: thm.text_secondary,
                                wrap: TextWrap::Wrap,
                            )
                        }
                        View(
                            width: w,
                            padding_top: 1,
                            flex_shrink: 0f32,
                        ) {
                            SelectList(
                                width: w,
                                height: 8u16,
                                options: options,
                                selected_index: Some(selected.clone()),
                                has_focus: hf,
                                show_description: false,
                                compact: true,
                                theme: Some(thm),
                            )
                        }
                    }
                }
            }
        }
        .into()
    }
}
