//! `/provider connect` dialog — OAuth / API-key provider connection flow.
//!
//! Multi-step dialog:
//!   1. **SelectAuthMethod** — choose between OAuth or API key authentication
//!   2. **SelectProvider** — pick a provider with fuzzy search (like the model selector)
//!   3. **OAuthSelect** — (OAuth only) select login method when the provider offers multiple options
//!   4. **OAuthDeviceCode** — (OAuth only) show device code URL and wait for authentication
//!   5. **EnterApiKey** — (API key only) type the API key in a dedicated dialog
//!
//! OAuth providers trigger the OAuth flow when OAuth authentication is selected.

use elph_ai::providers::builtin_providers;
use elph_ai::{builtin_oauth_provider_ids, get_builtin_providers};
use elph_tui::components::{DialogChrome, DialogUserInputContent, UiTheme, dialog_max_content_height};
use iocraft::prelude::*;

use crate::tui::slash_palette::list_viewport_cap;

use crate::tui::focus::ShellFocus;
use crate::tui::inline_dialog::{InlineDialogShell, OPTIONS_LIST_TOP_GAP, inline_body_width};
use crate::tui::slash_palette::fuzzy::{field_score, max_score};
use crate::utils::path::AppPaths;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

/// Default auth store path under `CONFIG_DIR/auth.json` (same as [`crate::platform::Paths`]).
///
/// Resolves via `ELPH_HOME` / XDG (`~/.config/elph/auth.json`). Never hardcodes `~/.elph/`.
pub fn default_auth_store_path() -> PathBuf {
    match crate::platform::Paths::resolve() {
        Ok(paths) => paths.auth_store_path(),
        Err(_) => {
            // Last-resort fallback aligned with PathResolver defaults (not ~/.elph/).
            let base = std::env::var_os("ELPH_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("elph")))
                .unwrap_or_else(|| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                    PathBuf::from(home).join(".config").join("elph")
                });
            base.join("auth.json")
        }
    }
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
    pub supports_api_key: bool,
    pub config_status: ProviderConfigStatus,
}

/// Configuration status of a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderConfigStatus {
    Unconfigured,
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
    OAuthSelect,
    EnterApiKey,
}

/// Authentication method selected in the first step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthMethod {
    Account,
    ApiKey,
}

/// Keyboard focus target within the provider dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderConnectFocus {
    #[default]
    AuthMethodList,
    Search,
    List,
    OAuthCodeInput,
    OAuthSelectList,
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
    /// Whether the current OAuth step is a text prompt (e.g. GitHub Enterprise domain)
    /// rather than a device code URL display.
    pub oauth_is_prompt: bool,
    /// Custom text prompt message (shown as body text when `oauth_is_prompt` is true).
    pub oauth_prompt_message: String,
    /// Labels for OAuth select options (e.g. ["Browser login (default)", "Device code login (headless)"]).
    pub oauth_select_labels: Vec<String>,
    /// IDs for OAuth select options (e.g. ["browser", "device_code"]).
    pub oauth_select_ids: Vec<String>,
    /// Selected index within the OAuth select options.
    pub oauth_select_index: usize,
    /// Provider ID to pre-select (from `/provider connect <id>`).
    pub provider_id: Option<String>,
    pub stashed_prompt_draft: Option<String>,
    /// Timestamp used to suppress accidental Enter repeats from the slash-submit key.
    pub opened_at: Instant,
    /// Set to true when OAuth flow completes — main loop will close the dialog.
    pub done: bool,
    /// True right after `open_provider_connect_dialog` sets up the dialog.
    /// Resets to false on the first step transition. The render function uses
    /// this to force the initial step to `SelectAuthMethod` regardless of any
    /// stale `step` value.
    pub fresh_open: bool,
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

pub fn provider_auth_method_from_index(index: usize) -> ProviderAuthMethod {
    if index == 0 {
        ProviderAuthMethod::Account
    } else {
        ProviderAuthMethod::ApiKey
    }
}

/// Check if a provider has configuration.
///
/// Only consults `auth.json` — providers not registered there are treated as
/// unconfigured even if an env var is set in the process environment.
/// Register env-var providers with: `elph provider connect <id> --env <VAR>`.
///
/// An `env:VAR` entry counts as configured for picker filtering even when the
/// process currently lacks that env var (the API call will fail later with a
/// clear error). Presence in `auth.json` is the source of truth for "configured".
fn get_provider_config_status(provider_id: &str) -> ProviderConfigStatus {
    get_provider_config_status_at(&default_auth_store_path(), provider_id)
}

/// Like [`get_provider_config_status`] but uses an explicit auth store path (tests / hosts).
pub fn get_provider_config_status_at(auth_store_path: &Path, provider_id: &str) -> ProviderConfigStatus {
    // Try loading the encrypted auth store first
    let file = match elph_agent::AuthStoreFile::load_from_path_sync(auth_store_path) {
        Ok(f) => f,
        Err(_) => {
            // Fallback: try to read as plain JSON (for manually created auth.json files)
            if let Ok(content) = std::fs::read_to_string(auth_store_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(providers) = json.get("providers").and_then(|v| v.as_object()) {
                        if let Some(credential) = providers.get(provider_id).and_then(|v| v.as_str()) {
                            if let Some(var_name) = credential.strip_prefix(elph_agent::ENV_REF_PREFIX) {
                                return ProviderConfigStatus::EnvVarConfigured(var_name.to_string());
                            }
                            // OAuth JSON blobs are long; short values are API keys.
                            if credential.trim().starts_with('{') || credential.len() > 100 {
                                return ProviderConfigStatus::OAuthConfigured;
                            }
                            return ProviderConfigStatus::ApiKeyConfigured;
                        }
                    }
                }
            }
            return ProviderConfigStatus::Unconfigured;
        }
    };

    if let Some(entry) = file.get_provider_credential(provider_id) {
        if let Some(var_name) = entry.strip_prefix(elph_agent::ENV_REF_PREFIX) {
            return ProviderConfigStatus::EnvVarConfigured(var_name.to_string());
        }
        // OAuth JSON blobs are long; short values are API keys.
        if entry.trim().starts_with('{') || entry.len() > 100 {
            return ProviderConfigStatus::OAuthConfigured;
        }
        return ProviderConfigStatus::ApiKeyConfigured;
    }
    ProviderConfigStatus::Unconfigured
}

/// Cached provider options to avoid recomputing on every render.
static CACHED_PROVIDER_OPTIONS: OnceLock<Vec<ProviderOption>> = OnceLock::new();

/// Get list of all providers with OAuth support info and configuration status.
pub fn get_provider_options() -> Vec<ProviderOption> {
    CACHED_PROVIDER_OPTIONS.get_or_init(|| {
        let oauth_provider_ids = builtin_oauth_provider_ids();
        let api_key_provider_ids: HashSet<String> = builtin_providers()
            .into_iter()
            .filter(|provider| provider.auth.api_key.is_some())
            .map(|provider| provider.id)
            .collect();

        get_builtin_providers()
            .into_iter()
            .map(|id| {
                let name = format_provider_name(&id);
                let supports_oauth = oauth_provider_ids.contains(&id.as_str());
                let supports_api_key = api_key_provider_ids.contains(&id);
                // Lazily compute config status only when needed (deferred to render time)
                ProviderOption {
                    name,
                    supports_oauth,
                    supports_api_key,
                    config_status: ProviderConfigStatus::Unconfigured,
                    id,
                }
            })
            .collect()
    })
    .clone()
}

pub fn providers_for_auth_method(providers: &[ProviderOption], auth_method: ProviderAuthMethod) -> Vec<ProviderOption> {
    providers
        .iter()
        .filter(|provider| match auth_method {
            ProviderAuthMethod::Account => provider.supports_oauth,
            ProviderAuthMethod::ApiKey => provider.supports_api_key,
        })
        .cloned()
        .collect()
}

pub fn get_provider_options_for_auth_method(auth_method: ProviderAuthMethod) -> Vec<ProviderOption> {
    providers_for_auth_method(&get_provider_options(), auth_method)
}

/// Format provider name for display.
///
/// Prefer the live factory label from `builtin_providers()` so new providers do not
/// need a separate hard-coded display map. Fall back to curated labels / title-case.
pub fn format_provider_name(id: &str) -> String {
    if let Some(provider) = builtin_providers().into_iter().find(|p| p.id == id)
        && !provider.name.is_empty()
        && provider.name != provider.id
    {
        return provider.name;
    }
    if let Some(cfg) = crate::agent::provider::provider_config(id) {
        return cfg.label.to_string();
    }
    match id {
        "faux" => "Faux".to_string(),
        _ => title_case_provider_id(id),
    }
}

fn title_case_provider_id(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

    let mut scored: Vec<(&ProviderOption, i32)> = providers
        .iter()
        .filter_map(|prov| provider_match_score(prov, query).map(|score| (prov, score)))
        .collect();

    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.name.cmp(&right.0.name)));

    scored.into_iter().map(|(prov, _)| prov.clone()).collect()
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
        oauth_select_labels: Vec::new(),
        oauth_select_ids: Vec::new(),
        oauth_select_index: 0,
        oauth_is_prompt: false,
        oauth_prompt_message: String::new(),
        provider_id: args.provider_id,
        stashed_prompt_draft: stashed,
        opened_at: Instant::now(),
        done: false,
        fresh_open: true,
    }));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

/// Close the provider connect dialog and restore stashed draft.
// TODO(refactor): group args into a parameter struct; WIP feature, suppress for now.
#[allow(clippy::too_many_arguments)]
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
#[allow(dead_code)]
pub enum ProviderFilterSeed {
    FocusOnly,
    Append(char),
}

/// Printable characters that seed the filter (like model selector).
#[allow(dead_code)]
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
#[allow(dead_code)]
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

// ── Viewport height (mirrors model_selector_list_viewport_height) ────

/// Fixed rows above the provider ModelOptionList (count label, search bar, paddings).
pub const PROVIDER_SELECT_LIST_FIXED_ROWS: u16 = 4;

/// Capped viewport height for the provider ModelOptionList, computed from terminal size.
pub fn provider_select_list_viewport_height(screen_width: u16, screen_height: u16) -> u16 {
    let theme = UiTheme::default();
    let chrome = DialogChrome::from_theme(theme, screen_width);
    let max_body = dialog_max_content_height(screen_height, &chrome, 12);
    (list_viewport_cap(screen_height).min(max_body.saturating_sub(PROVIDER_SELECT_LIST_FIXED_ROWS) as usize) as u16)
        .max(4)
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

    // Map to ModelRow + custom_hints for consistent rendering with provider step.
    let model_rows: Vec<crate::tui::model_selector::ModelRow> = auth_methods
        .iter()
        .map(|m| crate::tui::model_selector::ModelRow {
            value: m.name.clone(),
            name: m.name.clone(),
            provider_id: String::new(),
            model_id: m.name.clone(),
            context_k: 0,
            reasoning: false,
            images: false,
        })
        .collect();
    let desc_hints: Vec<String> = auth_methods.iter().map(|m| m.description.clone()).collect();

    let w = body_width;

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: "Authentication method".to_string(),
            has_focus: has_focus,
            footer_hint: Some(auth_method_footer()),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, gap: 0, flex_shrink: 0f32) {
                crate::tui::model_option_list::ModelOptionList(
                    width: w,
                    height: 0u16,
                    models: model_rows,
                    show_provider_hint: false,
                    custom_hints: desc_hints,
                    selected_index: Some(selected),
                    has_focus: has_focus,
                    theme: Some(theme),
                )
            }
        }
    }
    .into()
}

/// Render the provider connect dialog — dispatches to the correct step.
// TODO(refactor): group args into a parameter struct; WIP feature, suppress for now.
#[allow(clippy::too_many_arguments)]
pub fn render_provider_connect_dialog(
    screen_width: u16,
    screen_height: u16,
    has_focus: bool,
    selected: State<usize>,
    filter: State<String>,
    api_key_input: State<String>,
    selected_auth_method: usize,
    oauth_url: String,
    oauth_code: String,
    provider_name: String,
    step: ProviderConnectStep,
    input_focus: ProviderConnectFocus,
    fresh_open: bool,
    oauth_select_labels: Vec<String>,
    _oauth_select_index: usize,
    oauth_is_prompt: bool,
    oauth_prompt_message: String,
) -> AnyElement<'static> {
    // Defensive guard: ensure the dialog always starts at SelectAuthMethod.
    // The `fresh_open` flag is set by `open_provider_connect_dialog` and cleared
    // on the first step transition. This catches any stale step/input_focus
    // leakage between dialog open/close cycles.
    let step = if fresh_open && step != ProviderConnectStep::SelectAuthMethod {
        ProviderConnectStep::SelectAuthMethod
    } else {
        step
    };

    match step {
        ProviderConnectStep::SelectAuthMethod => {
            render_select_auth_method_step(screen_width, screen_height, has_focus, selected, input_focus)
        }
        ProviderConnectStep::SelectProvider => render_select_provider_step(
            screen_width,
            screen_height,
            has_focus,
            selected,
            filter,
            input_focus,
            selected_auth_method,
        ),
        ProviderConnectStep::OAuthDeviceCode => render_oauth_device_code_step(
            screen_width,
            screen_height,
            has_focus,
            oauth_url,
            oauth_code,
            provider_name,
            oauth_is_prompt,
            oauth_prompt_message,
        ),
        ProviderConnectStep::OAuthSelect => render_oauth_select_step(
            screen_width,
            screen_height,
            has_focus,
            oauth_select_labels.clone(),
            selected,
            input_focus,
        ),
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
    selected_auth_method: usize,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);

    let auth_method = provider_auth_method_from_index(selected_auth_method);
    let providers = get_provider_options_for_auth_method(auth_method);
    let filtered = filtered_providers(&providers, &filter.read());

    // Lazily compute config status only for filtered providers (performance optimization)
    let filtered_with_status: Vec<ProviderOption> = filtered
        .iter()
        .map(|p| {
            let config_status = get_provider_config_status(&p.id);
            ProviderOption {
                config_status,
                ..p.clone()
            }
        })
        .collect();

    let total_count = providers.len();
    let visible_count = filtered_with_status.len();
    let count_label = if visible_count < total_count {
        format!("{} of {} providers", visible_count, total_count)
    } else {
        format!("{} providers", total_count)
    };

    let search_focused = has_focus && input_focus == ProviderConnectFocus::Search;
    let list_focused = has_focus && input_focus == ProviderConnectFocus::List;
    let list_height = provider_select_list_viewport_height(screen_width, screen_height);

    let w = body_width;
    let thm = theme;

    // Map providers to ModelRow + custom_hints for ModelOptionList (same rendering
    // as the model selector — no "xx more" indicators, fixed viewport with overflow).
    let model_rows: Vec<crate::tui::model_selector::ModelRow> = filtered_with_status
        .iter()
        .map(|p| crate::tui::model_selector::ModelRow {
            value: p.id.clone(),
            name: p.name.clone(),
            provider_id: String::new(),
            model_id: p.name.clone(),
            context_k: 0,
            reasoning: false,
            images: false,
        })
        .collect();
    let config_hints: Vec<String> = filtered_with_status
        .iter()
        .map(|p| match &p.config_status {
            ProviderConfigStatus::Unconfigured => "unconfigured".into(),
            ProviderConfigStatus::ApiKeyConfigured => "API key configured".into(),
            ProviderConfigStatus::OAuthConfigured => "OAuth configured".into(),
            ProviderConfigStatus::EnvVarConfigured(var) => format!("env: {var}"),
        })
        .collect();

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: "Configure provider".to_string(),
            has_focus: has_focus,
            footer_hint: Some(provider_select_footer()),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, gap: 0, flex_shrink: 0f32) {
                // ── Count label (mirrors model selector) ──
                Text(
                    content: count_label,
                    color: thm.text_muted,
                    wrap: TextWrap::NoWrap,
                )
                // ── Search bar ──
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
                View(width: w, padding_top: OPTIONS_LIST_TOP_GAP, flex_shrink: 0f32) {
                    crate::tui::model_option_list::ModelOptionList(
                        width: w,
                        height: list_height,
                        models: model_rows,
                        show_provider_hint: false,
                        custom_hints: config_hints,
                        selected_index: Some(selected),
                        has_focus: list_focused,
                        theme: Some(thm),
                    )
                }
            }
        }
    }
    .into()
}

/// Render the OAuth select prompt step (e.g. OpenAI Codex login method selection).
fn render_oauth_select_step(
    screen_width: u16,
    _screen_height: u16,
    has_focus: bool,
    labels: Vec<String>,
    selected: State<usize>,
    input_focus: ProviderConnectFocus,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);

    let list_focused = has_focus && input_focus == ProviderConnectFocus::OAuthSelectList;

    // Map options to ModelRow for consistent rendering
    let model_rows: Vec<crate::tui::model_selector::ModelRow> = labels
        .iter()
        .map(|opt| crate::tui::model_selector::ModelRow {
            value: opt.clone(),
            name: opt.clone(),
            provider_id: String::new(),
            model_id: opt.clone(),
            context_k: 0,
            reasoning: false,
            images: false,
        })
        .collect();

    let w = body_width;
    let thm = theme;

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: "Select login method".to_string(),
            has_focus: has_focus,
            footer_hint: Some("↑↓ navigate · Enter confirm · Esc cancel".to_string()),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, gap: 0, flex_shrink: 0f32) {
                View(width: w, flex_shrink: 0f32) {
                    Text(
                        content: "Choose a login method:".to_string(),
                        color: thm.text_secondary,
                        wrap: TextWrap::NoWrap,
                    )
                }
                View(width: w, padding_top: OPTIONS_LIST_TOP_GAP, flex_shrink: 0f32) {
                    crate::tui::model_option_list::ModelOptionList(
                        width: w,
                        height: 0u16,
                        models: model_rows,
                        show_provider_hint: false,
                        custom_hints: vec![String::new(); labels.len()],
                        selected_index: Some(selected),
                        has_focus: list_focused,
                        theme: Some(thm),
                    )
                }
            }
        }
    }
    .into()
}

/// Render step 3: OAuth device code or text prompt.
#[allow(clippy::too_many_arguments)]
fn render_oauth_device_code_step(
    screen_width: u16,
    _screen_height: u16,
    has_focus: bool,
    oauth_url: String,
    oauth_code: String,
    provider_name: String,
    oauth_is_prompt: bool,
    oauth_prompt_message: String,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);

    let w = body_width;
    let thm = theme;

    let is_prompt =
        oauth_is_prompt || oauth_url.contains("GitHub Enterprise") || oauth_url.contains("enter value manually");

    let title = if oauth_is_prompt {
        format!("Login to {provider_name}")
    } else {
        format!("{provider_name} · OAuth")
    };

    let show_url = !oauth_is_prompt && !oauth_url.is_empty();
    let show_code = !oauth_is_prompt && !is_prompt && !oauth_code.is_empty();
    let show_input = oauth_is_prompt || is_prompt;

    let body_text = if oauth_is_prompt {
        oauth_prompt_message.clone()
    } else if is_prompt {
        "Enter the requested information:".to_string()
    } else {
        "Open the URL and enter the code:".to_string()
    };

    let status_text = if oauth_is_prompt || is_prompt {
        "Type your response and press Enter".to_string()
    } else {
        "Waiting for authentication…".to_string()
    };

    let url_text = oauth_url.clone();
    let code_text = if show_code {
        format!("Code: {oauth_code}")
    } else {
        String::new()
    };

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: title,
            has_focus: has_focus,
            footer_hint: Some(oauth_device_code_footer()),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                // Body text: prompt message or instruction
                View(width: w, flex_shrink: 0f32) {
                    Text(
                        content: body_text,
                        color: thm.text_secondary,
                        wrap: TextWrap::Wrap,
                    )
                }
                // URL (device code mode only, not prompt mode)
                View(width: w, padding_top: if show_url { 1 } else { 0 }, flex_shrink: 0f32) {
                    Text(content: url_text, color: thm.text_primary, weight: Weight::Bold, wrap: TextWrap::Wrap)
                }
                // Show device code when present (device code mode only)
                View(width: w, padding_top: if show_code { 1 } else { 0 }, flex_shrink: 0f32) {
                    Text(
                        content: code_text,
                        color: thm.accent_soft,
                        weight: Weight::Bold,
                        wrap: TextWrap::NoWrap,
                    )
                }
                // Text input line (only shown in prompt mode)
                View(width: w, padding_top: 0, flex_shrink: 0f32) {
                    Text(
                        content: if show_input {
                            if oauth_code.is_empty() { "> Enter text and press Enter to submit…".to_string() } else { format!("> {oauth_code}") }
                        } else {
                            String::new()
                        },
                        color: if has_focus { thm.text_primary } else { thm.text_muted },
                        wrap: TextWrap::NoWrap,
                    )
                }
                // Status message
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    Text(
                        content: status_text,
                        color: thm.text_muted,
                        wrap: TextWrap::NoWrap,
                    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, supports_oauth: bool, supports_api_key: bool) -> ProviderOption {
        ProviderOption {
            id: id.to_string(),
            name: id.to_string(),
            supports_oauth,
            supports_api_key,
            config_status: ProviderConfigStatus::Unconfigured,
        }
    }

    #[test]
    fn default_auth_store_path_uses_config_elph_not_dot_elph() {
        let path = default_auth_store_path();
        let s = path.to_string_lossy();
        // Must not use the legacy ~/.elph/auth.json location.
        assert!(!s.ends_with("/.elph/auth.json"), "legacy path still used: {s}");
        assert!(s.ends_with("auth.json"), "expected auth.json suffix, got {s}");
    }

    #[test]
    fn env_ref_in_auth_counts_as_configured_even_without_env() {
        let dir = tempfile::tempdir().expect("tempdir");
        let auth_path = dir.path().join("auth.json");
        let key = elph_agent::Aes256Key::generate();
        elph_agent::set_process_master_key_for_tests(key);
        let mut file = elph_agent::AuthStoreFile::default();
        file.set_provider_credential(
            "opencode",
            format!("{}{}", elph_agent::ENV_REF_PREFIX, "OPENCODE_API_KEY_DOES_NOT_EXIST_XYZ"),
        );
        elph_agent::try_block_on(async {
            // Uses process master key override for this test process.
            file.save_to_path(&auth_path).await.unwrap();
        })
        .expect("save sealed auth");
        // SAFETY: test-only env mutation; single-threaded unit test.
        unsafe {
            std::env::remove_var("OPENCODE_API_KEY_DOES_NOT_EXIST_XYZ");
        }
        let status = get_provider_config_status_at(&auth_path, "opencode");
        elph_agent::clear_process_master_key_for_tests();
        assert_eq!(
            status,
            ProviderConfigStatus::EnvVarConfigured("OPENCODE_API_KEY_DOES_NOT_EXIST_XYZ".into())
        );
        assert!(!matches!(status, ProviderConfigStatus::Unconfigured));
    }

    #[test]
    fn missing_auth_file_is_unconfigured() {
        let dir = tempfile::tempdir().expect("tempdir");
        let auth_path = dir.path().join("missing-auth.json");
        assert_eq!(
            get_provider_config_status_at(&auth_path, "opencode"),
            ProviderConfigStatus::Unconfigured
        );
    }

    #[test]
    fn auth_method_index_starts_with_account() {
        assert_eq!(provider_auth_method_from_index(0), ProviderAuthMethod::Account);
        assert_eq!(provider_auth_method_from_index(1), ProviderAuthMethod::ApiKey);
        assert_eq!(provider_auth_method_from_index(99), ProviderAuthMethod::ApiKey);
    }

    #[test]
    fn providers_for_auth_method_filters_by_supported_login_type() {
        let providers = vec![
            provider("oauth-only", true, false),
            provider("api-key-only", false, true),
            provider("both", true, true),
        ];

        let account_ids: Vec<_> = providers_for_auth_method(&providers, ProviderAuthMethod::Account)
            .into_iter()
            .map(|provider| provider.id)
            .collect();
        assert_eq!(account_ids, vec!["oauth-only", "both"]);

        let api_key_ids: Vec<_> = providers_for_auth_method(&providers, ProviderAuthMethod::ApiKey)
            .into_iter()
            .map(|provider| provider.id)
            .collect();
        assert_eq!(api_key_ids, vec!["api-key-only", "both"]);
    }

    #[test]
    fn provider_options_use_builtin_api_key_auth_metadata() {
        let options = get_provider_options();
        let openai_codex = options
            .iter()
            .find(|provider| provider.id == "openai-codex")
            .expect("openai-codex provider option");

        assert!(openai_codex.supports_oauth);
        assert!(!openai_codex.supports_api_key);
    }
}

// ── Provider disconnect dialog ────────────────────────────────────

/// Pending provider disconnect dialog state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingProviderDisconnectDialog {
    /// IDs of providers with stored credentials.
    pub provider_ids: Vec<String>,
    /// Selected index in the list.
    pub selected_index: usize,
    /// Provider ID to pre-select (from `/provider disconnect <id>`).
    pub provider_id: Option<String>,
    pub stashed_prompt_draft: Option<String>,
    pub opened_at: Instant,
    pub done: bool,
    /// Notification text to push to transcript when the dialog closes.
    pub notification_text: Option<String>,
}

/// Arguments for [`open_provider_disconnect_dialog`].
pub struct OpenProviderDisconnectDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingProviderDisconnectDialog>>,
    pub auth_store_path: &'a Path,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub provider_id: Option<String>,
}

/// Open the provider disconnect dialog showing providers with stored credentials.
pub fn open_provider_disconnect_dialog(args: OpenProviderDisconnectDialogArgs<'_>) {
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }

    let provider_ids = super::provider_credential_store::list_providers_with_credentials(args.auth_store_path);
    // Pre-select if a specific provider was given
    let selected_index = args
        .provider_id
        .as_ref()
        .and_then(|pid| provider_ids.iter().position(|id| id == pid))
        .unwrap_or(0);

    args.pending.set(Some(PendingProviderDisconnectDialog {
        provider_ids,
        selected_index,
        provider_id: args.provider_id,
        stashed_prompt_draft: stashed,
        opened_at: Instant::now(),
        done: false,
        notification_text: None,
    }));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

/// Close the provider disconnect dialog and restore stashed draft.
pub fn close_provider_disconnect_dialog(
    pending: &mut Ref<Option<PendingProviderDisconnectDialog>>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
    restore_stash: bool,
) {
    let stashed = pending.write().take().and_then(|p| p.stashed_prompt_draft);
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

/// Render the provider disconnect dialog.
pub fn render_provider_disconnect_dialog(
    screen_width: u16,
    _screen_height: u16,
    has_focus: bool,
    provider_ids: Vec<String>,
    selected_index: usize,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);

    let w = body_width;
    let thm = theme;

    let has_any = !provider_ids.is_empty();
    let count_label = if has_any {
        format!("{} provider(s) with stored credentials", provider_ids.len())
    } else {
        "No stored credentials found".to_string()
    };

    let footer = if has_any {
        "↑↓ navigate · Enter disconnect · Esc cancel".to_string()
    } else {
        "Esc cancel".to_string()
    };

    // Render rows with consistent selection styling
    let mut list_text = String::new();
    for (i, id) in provider_ids.iter().enumerate() {
        let selected = i == selected_index;
        let prefix = if selected { "❯ " } else { "  " };
        list_text.push_str(&format!("{}{}\n", prefix, format_provider_name(id)));
    }
    let list_text = list_text.trim_end().to_string();
    let (list_color, _list_weight) = if has_focus {
        (thm.text_primary, Weight::Normal)
    } else {
        (thm.text_muted, Weight::Normal)
    };

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: "Disconnect provider".to_string(),
            has_focus: has_focus,
            footer_hint: Some(footer),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                View(width: w, flex_shrink: 0f32) {
                    Text(
                        content: count_label,
                        color: thm.text_muted,
                        wrap: TextWrap::NoWrap,
                    )
                }
                View(width: w, padding_top: if has_any { OPTIONS_LIST_TOP_GAP } else { 0u16 }, flex_shrink: 0f32) {
                    Text(
                        content: list_text,
                        color: list_color,
                        wrap: TextWrap::NoWrap,
                    )
                }
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    Text(
                        content: if has_any {
                            "Select a provider and press Enter to remove its stored credentials."
                        } else {
                            ""
                        }.to_string(),
                        color: thm.text_muted,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
        }
    }
    .into()
}
