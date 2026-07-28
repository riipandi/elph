//! `/provider connect` dialog — OAuth / API-key provider connection flow.
//!
//! Two-step dialog:
//!   1. **SelectProvider** — pick a provider with fuzzy search (like the model selector).
//!   2. **EnterApiKey** — (non-OAuth only) type the API key in a dedicated dialog.

use elph_ai::{builtin_oauth_provider_ids, get_builtin_providers};
use elph_tui::components::{DialogUserInputContent, SelectList, UiTheme};
use elph_tui::types::SelectOption;
use iocraft::prelude::*;

use crate::tui::focus::ShellFocus;
use crate::tui::inline_dialog::{InlineDialogShell, inline_body_width};
use crate::tui::slash_palette::fuzzy::{field_score, max_score};

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
}

/// Dialog step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderConnectStep {
    SelectProvider,
    EnterApiKey,
}

/// Keyboard focus target within the provider dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderConnectFocus {
    #[default]
    Search,
    List,
    ApiKeyInput,
}

/// Pending provider connection dialog state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingProviderConnectDialog {
    pub step: ProviderConnectStep,
    pub selected_provider: usize,
    pub filter: String,
    pub input_focus: ProviderConnectFocus,
    pub api_key_input: String,
    /// Provider ID to pre-select (from `/provider connect <id>`).
    pub provider_id: Option<String>,
    pub stashed_prompt_draft: Option<String>,
}

/// Pending API key input dialog state (separate from provider selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingProviderApiKeyDialog {
    pub provider_id: String,
    pub provider_name: String,
    pub stashed_prompt_draft: Option<String>,
}

// ── Provider data helpers ────────────────────────────────────────────

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

/// Open the provider connect dialog (step 1: SelectProvider).
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
    let initial_selected = args
        .provider_id
        .as_ref()
        .and_then(|id| provider_options.iter().position(|p| p.id == *id))
        .unwrap_or(0);

    args.selected.set(initial_selected);
    args.filter.set(String::new());
    args.api_key_input.set(String::new());
    args.input_focus.set(ProviderConnectFocus::List);

    args.pending.set(Some(PendingProviderConnectDialog {
        step: ProviderConnectStep::SelectProvider,
        selected_provider: initial_selected,
        filter: String::new(),
        input_focus: ProviderConnectFocus::List,
        api_key_input: String::new(),
        provider_id: args.provider_id,
        stashed_prompt_draft: stashed,
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

/// List navigation delta for `↑/↓` or `k/j`.
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
    focus == ProviderConnectFocus::List
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

fn provider_select_footer_hint() -> String {
    "↑↓ move · / search · Enter select · Esc cancel".to_string()
}

/// Render the provider connect dialog — dispatches to the correct step.
pub fn render_provider_connect_dialog(
    screen_width: u16,
    has_focus: bool,
    selected: State<usize>,
    filter: State<String>,
    api_key_input: State<String>,
    step: ProviderConnectStep,
    input_focus: ProviderConnectFocus,
) -> AnyElement<'static> {
    match step {
        ProviderConnectStep::SelectProvider => {
            render_select_provider_step(screen_width, has_focus, selected, filter, input_focus)
        }
        ProviderConnectStep::EnterApiKey => render_api_key_step(screen_width, has_focus, api_key_input),
    }
}

/// Render step 1: provider selection with fuzzy search.
fn render_select_provider_step(
    screen_width: u16,
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
            let suffix = if p.supports_oauth { " (OAuth)" } else { " (API Key)" };
            SelectOption::new(&format!("{}{}", p.name, suffix), &p.id)
        })
        .collect();

    // Re-clamp selected index
    let _safe_selected = clamp_selected(selected.get(), filtered.len());
    let total_count = providers.len();
    let visible_count = filtered.len();
    let count_label = if visible_count < total_count {
        format!("{} of {} providers", visible_count, total_count)
    } else {
        format!("{} providers", total_count)
    };

    // Like model_selector_bar: visually distinguish search vs list focus
    let search_focused = has_focus && input_focus == ProviderConnectFocus::Search;
    let list_focused = has_focus && input_focus == ProviderConnectFocus::List;

    let w = body_width;
    let thm = theme;

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: "Connect Provider".to_string(),
            has_focus: has_focus,
            footer_hint: Some(provider_select_footer_hint()),
        ) {
            View(width: w, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                // ── Search bar ──
                View(width: w, flex_shrink: 0f32) {
                    DialogUserInputContent(
                        width: w,
                        value: Some(filter.clone()),
                        has_focus: search_focused,
                        theme: Some(thm),
                        compact: true,
                        show_prompt: true,
                        question: "Search provider:".to_string(),
                        show_footer_hint: false,
                        dialog_chrome: true,
                        on_submit: HandlerMut::default(),
                        on_cancel: HandlerMut::default(),
                    )
                }
                // ── Count label ──
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    Text(content: count_label, color: thm.text_hint, wrap: TextWrap::NoWrap)
                }
                // ── Provider list ──
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
                    SelectList(
                        width: w,
                        height: 8u16,
                        options: options,
                        selected_index: Some(selected.clone()),
                        has_focus: list_focused,
                        show_description: false,
                        compact: true,
                        theme: Some(thm),
                    )
                }
            }
        }
    }
    .into()
}

/// Render step 2: dedicated API key input dialog.
fn render_api_key_step(screen_width: u16, has_focus: bool, api_key_input: State<String>) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(screen_width);

    // We don't know the provider name here without the pending state, but the
    // footer hint is set dynamically when rendering. We use a generic label.
    let w = body_width;
    let hf = has_focus;
    let thm = theme;

    element! {
        InlineDialogShell(
            screen_width: screen_width,
            title: "Enter API Key".to_string(),
            has_focus: has_focus,
            footer_hint: None::<String>,
        ) {
            View(width: w, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                View(width: w, flex_shrink: 0f32) {
                    Text(
                        content: "Paste or type your API key below:".to_string(),
                        color: thm.text_secondary,
                        wrap: TextWrap::Wrap,
                    )
                }
                View(width: w, padding_top: 1, flex_shrink: 0f32) {
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
    .into()
}
