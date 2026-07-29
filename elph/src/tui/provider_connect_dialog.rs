//! `/provider connect` dialog — OAuth / API-key provider connection flow.
//!
//! Multi-step dialog:
//!   1. **SelectAuthMethod** — choose between OAuth or API key authentication
//!   2. **SelectProvider** — pick a provider with fuzzy search (like the model selector)
//!   3. **OAuthDeviceCode** — (OAuth only) show device code URL and wait for authentication
//!   4. **EnterApiKey** — (API key only) type the API key in a dedicated dialog
//!
//! OAuth providers trigger the OAuth flow when OAuth authentication is selected.

use elph_ai::{builtin_oauth_provider_ids, get_builtin_providers};
use elph_tui::components::{DialogUserInputContent, SelectList, UiTheme};
use elph_tui::types::SelectOption;
use iocraft::prelude::*;

use crate::tui::focus::ShellFocus;
use crate::tui::inline_dialog::{InlineDialogShell, inline_body_width};
use crate::tui::model_selector::model_selector_list_viewport_height;
use crate::tui::provider_credential_store::has_provider_credential;
use crate::tui::slash_palette::fuzzy::{field_score, max_score};

use std::path::PathBuf;

/// Default auth store path under `~/.elph/auth.json`.
fn default_auth_store_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".elph").join("auth.json")
}

// ── Data types ───────────────────────────────────────────────────────

const NAME_WEIGHT: i32 = 4;
const ID_WEIGHT: i32 = 3;
const SUPPLIER_WEIGHT: i32 = 1;

/// Provider information for the selection list.
#[derive(Debug, Clone)]
pub struct ProviderOption {
    pub id: String,
    pub name: String,
    pub supports_oauth: bool,
    pub config_status: ProviderConfigStatus,
}

/// Configuration status of a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderConfigStatus {
    Unconfigured,
    #[allow(dead_code)]
    ApiKeyConfigured,
    OAuthConfigured,
    EnvVarConfigured(String), // Environment variable name
}

/// Dialog step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderConnectStep {
    SelectAuthMethod,
    SelectProvider,
    OAuthDeviceCode,
    EnterApiKey,
}

/// Keyboard focus target within the provider dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderConnectFocus {
    #[default]
    AuthMethodList,
    Search,
    List,
    OAuthCodeInput,
    ApiKeyInput,
}

/// Pending provider connection dialog state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingProviderConnectDialog {
    pub step: ProviderConnectStep,
    pub selected_provider: usize,
    pub selected_auth_method: usize,
    pub filter: String,
    pub input_focus: ProviderConnectFocus,
    pub api_key_input: String,
    pub oauth_code: String,
    pub oauth_url: String,
    pub oauth_provider_name: String,
    /// Provider ID to pre-select (from `/provider connect <id>`).
    pub provider_id: Option<String>,
    pub stashed_prompt_draft: Option<String>,
    /// Set to true when OAuth flow completes — main loop will close the dialog.
    pub done: bool,
}

/// Pending API key input dialog state (separate from provider selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingProviderApiKeyDialog {
    pub provider_id: String,
    pub provider_name: String,
    pub stashed_prompt_draft: Option<String>,
}

// ── Provider data helpers ────────────────────────────────────────────

/// Authentication method options.
#[derive(Debug, Clone)]
pub struct AuthMethodOption {
    pub name: String,
    #[allow(dead_code)]
    pub description: String,
}

/// Get available authentication methods.
pub fn get_auth_methods() -> Vec<AuthMethodOption> {
    vec![
        AuthMethodOption {
            name: "Sign in with an account".to_string(),
            description: "OAuth login for supported providers".to_string(),
        },
        AuthMethodOption {
            name: "Sign in with an API key".to_string(),
            description: "Manually enter an API key".to_string(),
        },
    ]
}

/// Check if a provider has configuration.
fn get_provider_config_status(provider_id: &str) -> ProviderConfigStatus {
    // Check environment variables first
    let env_var = match provider_id {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "openai-codex" => "OPENAI_API_KEY",
        "github-copilot" => "GITHUB_TOKEN",
        "hyper" => "HYPER_API_KEY",
        "xai" => "XAI_API_KEY",
        "google" => "GOOGLE_API_KEY",
        "google-vertex" => "GOOGLE_VERTEX_API_KEY",
        "amazon-bedrock" => "AWS_ACCESS_KEY_ID",
        "cloudflare-ai-gateway" => "CLOUDFLARE_API_TOKEN",
        "cloudflare-workers-ai" => "CLOUDFLARE_API_TOKEN",
        "fireworks" => "FIREWORKS_API_KEY",
        "groq" => "GROQ_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        "huggingface" => "HUGGINGFACE_API_KEY",
        "kimi-coding" => "KIMI_API_KEY",
        "xiaomi" => "XIAOMI_API_KEY",
        "zai" => "ZAI_API_KEY",
        "cerebras" => "CEREBRAS_API_KEY",
        "kilo" => "KILO_API_KEY",
        "faux" => "FAUX_API_KEY",
        _ => return check_stored_credential(provider_id),
    };

    if std::env::var(env_var).is_ok() {
        return ProviderConfigStatus::EnvVarConfigured(env_var.to_string());
    }

    check_stored_credential(provider_id)
}

/// Check auth.json for any stored credential (API key or OAuth token).
fn check_stored_credential(provider_id: &str) -> ProviderConfigStatus {
    // Normalize: strip legacy "oauth:" prefix if present
    let normalized = provider_id.strip_prefix("oauth:").unwrap_or(provider_id);

    if !has_stored_api_key(normalized) {
        // Also check legacy prefixed key
        let prefixed = format!("oauth:{normalized}");
        if has_stored_api_key(&prefixed) {
            // Found with prefixed key — treat as configured (migration on next save)
            return ProviderConfigStatus::ApiKeyConfigured;
        }
        return ProviderConfigStatus::Unconfigured;
    }

    // Determine the type of stored credential (API key vs OAuth blob).
    let auth_store_path = default_auth_store_path();
    if !auth_store_path.exists() {
        return ProviderConfigStatus::Unconfigured;
    }
    if let Ok(bytes) = std::fs::read(&auth_store_path) {
        if let Ok(file) = serde_json::from_slice::<elph_agent::AuthStoreFile>(&bytes) {
            if let Some(enc) = file.get_provider_credential(normalized) {
                // If it's encrypted, we can't tell the type without decrypting.
                // Use a heuristic: API keys are typically short strings, OAuth blobs are longer JSON.
                // For safety, check if the credential was stored via OAuth flow by looking it up.
                if enc.len() > 100 {
                    // Longer encrypted values likely contain OAuth credential JSON
                    return ProviderConfigStatus::OAuthConfigured;
                }
                return ProviderConfigStatus::ApiKeyConfigured;
            }

            // Also check legacy prefixed key as fallback
            let prefixed = format!("oauth:{normalized}");
            if let Some(_enc) = file.get_provider_credential(&prefixed) {
                return ProviderConfigStatus::OAuthConfigured;
            }
        }
    }

    ProviderConfigStatus::Unconfigured
}

/// Check if provider has stored API key in auth.json
fn has_stored_api_key(provider_id: &str) -> bool {
    let auth_store_path = default_auth_store_path();
    has_provider_credential(&auth_store_path, provider_id)
}

/// Get the auth store path for provider credentials.
fn auth_store_path() -> std::path::PathBuf {
    default_auth_store_path()
}

/// Save API key to auth.json with encryption
pub async fn save_provider_api_key(provider_id: &str, api_key: String) -> anyhow::Result<()> {
    let path = auth_store_path();
    crate::tui::provider_credential_store::save_provider_credential(&path, provider_id, &api_key).await
}

/// Load and decrypt API key from auth.json
pub async fn load_provider_api_key(provider_id: &str) -> anyhow::Result<Option<String>> {
    let path = auth_store_path();
    crate::tui::provider_credential_store::load_provider_credential(&path, provider_id).await
}

/// Remove stored API key for a provider
pub async fn remove_provider_api_key(provider_id: &str) -> anyhow::Result<bool> {
    let path = auth_store_path();
    crate::tui::provider_credential_store::remove_provider_credential(&path, provider_id).await
}

/// Get list of all providers with OAuth support info and configuration status.
pub fn get_provider_options() -> Vec<ProviderOption> {
    let oauth_provider_ids = builtin_oauth_provider_ids();

    get_builtin_providers()
        .into_iter()
        .map(|id| ProviderOption {
            id: id.to_string(),
            name: format_provider_name(id),
            supports_oauth: oauth_provider_ids.contains(&id),
            config_status: get_provider_config_status(id),
        })
        .collect()
}

/// Format provider name for display.
pub fn format_provider_name(id: &str) -> String {
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

/// Check if provider supports OAuth.
pub fn provider_supports_oauth(provider_id: &str) -> bool {
    builtin_oauth_provider_ids().contains(&provider_id)
}

// ── Fuzzy filtering (adapted from model_selector) ────────────────────

fn provider_match_score(option: &ProviderOption, query: &str) -> Option<i32> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Some(0);
    }

    let mut best = field_score(&query, &option.name.to_ascii_lowercase(), NAME_WEIGHT, true);
    best = max_score(best, field_score(&query, &option.id.to_ascii_lowercase(), ID_WEIGHT, true));

    if option.supports_oauth {
        best = max_score(best, field_score(&query, "oauth oauth2 sso", SUPPLIER_WEIGHT, false));
    } else {
        best = max_score(best, field_score(&query, "key api key apikey", SUPPLIER_WEIGHT, false));
    }

    best
}

pub fn filtered_providers(providers: &[ProviderOption], filter: &str) -> Vec<ProviderOption> {
    let query = filter.trim();
    if query.is_empty() {
        return providers.to_vec();
    }

    let mut scored: Vec<(ProviderOption, i32)> = providers
        .iter()
        .filter_map(|prov| provider_match_score(prov, query).map(|score| (prov.clone(), score)))
        .collect();

    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.name.cmp(&right.0.name)));

    scored.into_iter().map(|(prov, _)| prov).collect()
}

fn clamp_selected(selected: usize, count: usize) -> usize {
    if count == 0 {
        0
    } else {
        selected.min(count.saturating_sub(1))
    }
}

// ── Dialog lifecycle functions ───────────────────────────────────────

/// Arguments for [`open_provider_connect_dialog`].
pub struct OpenProviderConnectDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingProviderConnectDialog>>,
    pub selected: &'a mut State<usize>,
    pub filter: &'a mut State<String>,
    pub api_key_input: &'a mut State<String>,
    pub input_focus: &'a mut State<ProviderConnectFocus>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub provider_id: Option<String>,
}

/// Open the provider connect dialog (step 1: SelectAuthMethod).
pub fn open_provider_connect_dialog(args: OpenProviderConnectDialogArgs<'_>) {
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }

    args.selected.set(0);
    args.filter.set(String::new());
    args.api_key_input.set(String::new());
    args.input_focus.set(ProviderConnectFocus::AuthMethodList);

    args.pending.set(Some(PendingProviderConnectDialog {
        step: ProviderConnectStep::SelectAuthMethod,
        selected_provider: 0,
        selected_auth_method: 0,
        filter: String::new(),
        input_focus: ProviderConnectFocus::AuthMethodList,
        api_key_input: String::new(),
        oauth_code: String::new(),
        oauth_url: String::new(),
        oauth_provider_name: String::new(),
        provider_id: args.provider_id,
        stashed_prompt_draft: stashed,
        done: false,
    }));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

/// Transition from SelectProvider to EnterApiKey step.
#[allow(dead_code)]
pub fn transition_to_api_key_step(pending: &mut PendingProviderConnectDialog, providers: &[ProviderOption]) {
    let filtered = filtered_providers(providers, &pending.filter);
    if let Some(provider) = filtered.get(pending.selected_provider) {
        if !provider.supports_oauth {
            pending.step = ProviderConnectStep::EnterApiKey;
            pending.input_focus = ProviderConnectFocus::ApiKeyInput;
            pending.api_key_input.clear();
        }
    }
}

/// Transition from EnterApiKey back to SelectProvider step.
#[allow(dead_code)]
pub fn transition_to_select_provider_step(pending: &mut PendingProviderConnectDialog) {
    pending.step = ProviderConnectStep::SelectProvider;
    pending.input_focus = ProviderConnectFocus::List;
    pending.api_key_input.clear();
}

/// Close the provider connect dialog and restore stashed draft.
pub fn close_provider_connect_dialog(
    pending: &mut Ref<Option<PendingProviderConnectDialog>>,
    selected: &mut State<usize>,
    filter: &mut State<String>,
    api_key_input: &mut State<String>,
    input_focus: &mut State<ProviderConnectFocus>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
    restore_stash: bool,
) {
    let stashed = pending.write().take().and_then(|p| p.stashed_prompt_draft);
    selected.set(0);
    filter.set(String::new());
    api_key_input.set(String::new());
    input_focus.set(ProviderConnectFocus::default());

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

/// Arguments for [`open_provider_api_key_dialog`].
pub struct OpenProviderApiKeyDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingProviderApiKeyDialog>>,
    pub api_key_input: &'a mut State<String>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub provider_id: String,
    pub provider_name: String,
}

/// Open a dedicated API key input dialog (separate from the provider selector).
pub fn open_provider_api_key_dialog(args: OpenProviderApiKeyDialogArgs<'_>) {
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }

    args.api_key_input.set(String::new());

    args.pending.set(Some(PendingProviderApiKeyDialog {
        provider_id: args.provider_id,
        provider_name: args.provider_name,
        stashed_prompt_draft: stashed,
    }));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

/// Close the API key dialog and restore stashed draft.
pub fn close_provider_api_key_dialog(
    pending: &mut Ref<Option<PendingProviderApiKeyDialog>>,
    api_key_input: &mut State<String>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
    restore_stash: bool,
) {
    let stashed = pending.write().take().and_then(|p| p.stashed_prompt_draft);
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

/// Get the provider at a given index from the **filtered** list.
pub fn get_filtered_provider_at(providers: &[ProviderOption], filter: &str, index: usize) -> Option<ProviderOption> {
    filtered_providers(providers, filter).get(index).cloned()
}

/// Count providers matching the filter.
pub fn count_filtered(providers: &[ProviderOption], filter: &str) -> usize {
    filtered_providers(providers, filter).len()
}

// ── Keyboard helpers (analogous to model_selector_shell) ─────────────

/// How a list keystroke seeds the filter field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFilterSeed {
    FocusOnly,
    Append(char),
}

/// Printable characters that seed the filter (like model selector).
pub fn provider_filter_seed(modifiers: KeyModifiers, code: KeyCode) -> Option<ProviderFilterSeed> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char('/') => Some(ProviderFilterSeed::FocusOnly),
        KeyCode::Char(' ') => Some(ProviderFilterSeed::Append(' ')),
        KeyCode::Char(c)
            if (c.is_ascii_alphabetic() || c.is_ascii_digit())
                && !matches!(c, 'h' | 'j' | 'k' | 'l' | 'H' | 'J' | 'K' | 'L') =>
        {
            Some(ProviderFilterSeed::Append(c))
        }
        _ => None,
    }
}

/// List navigation delta for `↑/↓` or `k/j` (works for both auth method and provider lists).
pub fn provider_list_nav_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Up | KeyCode::Char('k') => Some(-1),
        KeyCode::Down | KeyCode::Char('j') => Some(1),
        _ => None,
    }
}

/// Focus the filter / search field (used by shell keyboard handler).
pub fn focus_provider_search(
    input_focus: &mut State<ProviderConnectFocus>,
    pending: &mut PendingProviderConnectDialog,
) {
    input_focus.set(ProviderConnectFocus::Search);
    pending.input_focus = ProviderConnectFocus::Search;
}

/// Focus the provider list (used by shell keyboard handler).
pub fn focus_provider_list(input_focus: &mut State<ProviderConnectFocus>, pending: &mut PendingProviderConnectDialog) {
    input_focus.set(ProviderConnectFocus::List);
    pending.input_focus = ProviderConnectFocus::List;
}

/// Only confirm a provider selection when focus is on the list (not the search field).
pub fn provider_confirm_on_enter(focus: ProviderConnectFocus) -> bool {
    matches!(focus, ProviderConnectFocus::List | ProviderConnectFocus::AuthMethodList)
}

/// Apply a filter seed keystroke: focus search, optionally append a character.
pub fn apply_provider_filter_seed(
    seed: ProviderFilterSeed,
    filter: &mut State<String>,
    input_focus: &mut State<ProviderConnectFocus>,
    pending: &mut PendingProviderConnectDialog,
) {
    focus_provider_search(input_focus, pending);
    if let ProviderFilterSeed::Append(ch) = seed {
        let mut next = filter.read().clone();
        next.push(ch);
        filter.set(next.clone());
        pending.filter = next;
    }
}

// ── Rendering ────────────────────────────────────────────────────────

fn auth_method_footer() -> String {
    "↑↓ move · Enter select · Esc cancel".to_string()
}

fn provider_select_footer() -> String {
    "↑↓ navigate · / filter · Enter confirm · Esc cancel".to_string()
}

fn oauth_device_code_footer() -> String {
    "Ctrl+O open · Esc cancel".to_string()
}

fn api_key_footer(provider_name: &str) -> String {
    format!("Enter confirm · Esc cancel · Provider: {provider_name}")
}

/// Render step 1: authentication method selection.
fn render_select_auth_method_step(
    screen_width: u16,
    _screen_height: u16,
    has_focus: bool,
    selected: State<usize>,
    _input_focus: ProviderConnectFocus,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);

    let auth_methods = get_auth_methods();
    let options: Vec<SelectOption> = auth_methods
        .iter()
        .map(|m| SelectOption::new(&m.name, ""))
        .collect();

    let w = body_width;

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: "Authentication method".to_string(),
            has_focus: has_focus,
            footer_hint: Some(auth_method_footer()),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                View(width: w, flex_shrink: 0f32) {
                    SelectList(
                        width: w,
                        height: 0u16,
                        options: options,
                        selected_index: Some(selected),
                        has_focus: has_focus,
                        show_description: false,
                        compact: true,
                        theme: Some(theme),
                    )
                }
            }
        }
    }
    .into()
}

/// Render the provider connect dialog — dispatches to the correct step.
pub fn render_provider_connect_dialog(
    screen_width: u16,
    screen_height: u16,
    has_focus: bool,
    selected: State<usize>,
    filter: State<String>,
    api_key_input: State<String>,
    oauth_url: String,
    oauth_code: String,
    provider_name: String,
    step: ProviderConnectStep,
    input_focus: ProviderConnectFocus,
) -> AnyElement<'static> {
    match step {
        ProviderConnectStep::SelectAuthMethod => {
            render_select_auth_method_step(screen_width, screen_height, has_focus, selected, input_focus)
        }
        ProviderConnectStep::SelectProvider => {
            render_select_provider_step(screen_width, screen_height, has_focus, selected, filter, input_focus)
        }
        ProviderConnectStep::OAuthDeviceCode => {
            render_oauth_device_code_step(screen_width, screen_height, has_focus, oauth_url, oauth_code, provider_name)
        }
        ProviderConnectStep::EnterApiKey => {
            render_api_key_step(screen_width, screen_height, has_focus, api_key_input, provider_name)
        }
    }
}

/// Render step 2: provider selection with fuzzy search.
fn render_select_provider_step(
    screen_width: u16,
    screen_height: u16,
    has_focus: bool,
    selected: State<usize>,
    filter: State<String>,
    input_focus: ProviderConnectFocus,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);

    let providers = get_provider_options();
    let filtered = filtered_providers(&providers, &filter.read());

    let options: Vec<SelectOption> = filtered
        .iter()
        .map(|p| {
            let description = match &p.config_status {
                ProviderConfigStatus::Unconfigured => "unconfigured".to_string(),
                ProviderConfigStatus::ApiKeyConfigured => "key configured".to_string(),
                ProviderConfigStatus::OAuthConfigured => "OAuth configured".to_string(),
                ProviderConfigStatus::EnvVarConfigured(var) => var.clone(),
            };
            let description_color = match &p.config_status {
                ProviderConfigStatus::Unconfigured => Some(theme.text_hint),
                _ => Some(theme.text_muted),
            };
            SelectOption {
                name: p.name.clone(),
                description,
                description_color,
            }
        })
        .collect();

    let viewport_cap = model_selector_list_viewport_height(screen_width, screen_height) as usize;
    let list_height = viewport_cap as u16;
    let total_count = providers.len();
    let visible_count = filtered.len();
    let count_label = if visible_count < total_count {
        format!("{} of {} providers", visible_count, total_count)
    } else {
        format!("{} providers", total_count)
    };

    let search_focused = has_focus && input_focus == ProviderConnectFocus::Search;
    let list_focused = has_focus && input_focus == ProviderConnectFocus::List;

    let w = body_width;
    let thm = theme;

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: "Configure provider".to_string(),
            has_focus: has_focus,
            footer_hint: Some(provider_select_footer()),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                // ── Count label ──
                View(width: w, flex_shrink: 0f32) {
                    Text(content: count_label, color: thm.text_muted, wrap: TextWrap::NoWrap)
                }
                // ── Search bar (matches model selector pattern) ──
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    DialogUserInputContent(
                        width: w,
                        value: Some(filter),
                        has_focus: search_focused,
                        theme: Some(thm),
                        compact: true,
                        show_prompt: false,
                        placeholder: "Filter providers…".to_string(),
                        show_placeholder_when_focused: true,
                        show_footer_hint: false,
                        dialog_chrome: true,
                        on_submit: HandlerMut::default(),
                        on_cancel: HandlerMut::default(),
                    )
                }
                // ── Provider list ──
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    SelectList(
                        width: w,
                        height: list_height,
                        options: options,
                        selected_index: Some(selected.clone()),
                        has_focus: list_focused,
                        show_description: true,
                        inline_description: true,
                        compact: true,
                        theme: Some(thm),
                    )
                }
            }
        }
    }
    .into()
}

/// Render step 3: OAuth device code.
fn render_oauth_device_code_step(
    screen_width: u16,
    _screen_height: u16,
    has_focus: bool,
    oauth_url: String,
    oauth_code: String,
    provider_name: String,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);

    let w = body_width;
    let thm = theme;

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: format!("{provider_name} · OAuth"),
            has_focus: has_focus,
            footer_hint: Some(oauth_device_code_footer()),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                View(width: w, flex_shrink: 0f32) {
                    Text(content: "Open the URL and enter the code:".to_string(), color: thm.text_secondary, wrap: TextWrap::Wrap)
                }
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    Text(content: oauth_url, color: thm.text_primary, weight: Weight::Bold, wrap: TextWrap::Wrap)
                }
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    Text(content: format!("Code: {oauth_code}"), color: thm.accent_soft, weight: Weight::Bold, wrap: TextWrap::NoWrap)
                }
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    Text(content: "Waiting for authentication…".to_string(), color: thm.text_muted, wrap: TextWrap::NoWrap)
                }
            }
        }
    }
    .into()
}

/// Render step 4: API key input.
fn render_api_key_step(
    screen_width: u16,
    _screen_height: u16,
    has_focus: bool,
    api_key_input: State<String>,
    provider_name: String,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);

    let w = body_width;
    let hf = has_focus;
    let thm = theme;

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: format!("API key · {provider_name}"),
            has_focus: has_focus,
            footer_hint: Some(api_key_footer(&provider_name)),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                View(width: w, flex_shrink: 0f32) {
                    Text(
                        content: format!("Paste your API key for {provider_name}:"),
                        color: thm.text_secondary,
                        wrap: TextWrap::Wrap,
                    )
                }
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    DialogUserInputContent(
                        width: w,
                        placeholder: format!("sk-… ({provider_name} API key)"),
                        value: Some(api_key_input),
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
    .into()
}
