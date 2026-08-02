//! Model picker state, catalog snapshot, and fuzzy filtering.

use std::collections::{HashMap, HashSet};

use elph_ai::Model;
use elph_ai::{get_builtin_model, get_builtin_models, get_builtin_providers};
use elph_tui::components::{DialogChrome, UiTheme, dialog_max_content_height};

use crate::agent::parse_model_value;
use crate::utils::path::AppPaths;

use super::slash_palette::fuzzy::{field_score, max_score};
use super::slash_palette::list_viewport_cap;

const NAME_WEIGHT: i32 = 4;
const ID_WEIGHT: i32 = 3;
const DESCRIPTION_WEIGHT: i32 = 1;

/// Synthetic provider id for the aggregate "All" tab (index 0).
pub const ALL_PROVIDERS_TAB_ID: &str = "__all__";

/// Tab index for the aggregate "All providers" view.
pub const ALL_PROVIDERS_TAB_INDEX: usize = 0;

/// Header label for [`ALL_PROVIDERS_TAB_INDEX`].
pub const ALL_PROVIDERS_TAB_LABEL: &str = "All";

/// Synthetic provider id for the Free tab (index 1).
pub const FREE_PROVIDERS_TAB_ID: &str = "__free__";

/// Tab index for free models.
pub const FREE_PROVIDERS_TAB_INDEX: usize = 1;

/// Header label for [`FREE_PROVIDERS_TAB_INDEX`].
pub const FREE_PROVIDERS_TAB_LABEL: &str = "Free";

/// Synthetic provider id for the curated Scoped tab (index 2).
pub const SCOPED_PROVIDERS_TAB_ID: &str = "__scoped__";

/// Tab index for settings-backed scoped models.
pub const SCOPED_PROVIDERS_TAB_INDEX: usize = 2;

/// Header label for [`SCOPED_PROVIDERS_TAB_INDEX`].
pub const SCOPED_PROVIDERS_TAB_LABEL: &str = "Scoped";

/// Header label for the Provider scope mode (fourth scope tab).
pub const PROVIDER_SCOPE_TAB_LABEL: &str = "Provider";

/// Index of the Provider scope tab in the 4-tab header (`All` · `Free` · `Scoped` · `Provider`).
pub const PROVIDER_SCOPE_TAB_INDEX: usize = 3;

/// Number of scope tabs shown in the model picker header.
pub const SCOPE_TAB_COUNT: usize = 4;

/// Built-in provider tabs shown per header page (remaining tabs scroll via `‹ N` / `N ›`).
pub const PROVIDER_HEADER_TABS_PER_PAGE: usize = 4;

/// Catalog index where built-in provider tabs begin (after synthetic All/Free/Scoped tabs).
pub const BUILTIN_PROVIDERS_START_INDEX: usize = 3;

/// Scope filter for the model picker header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelScopeMode {
    All,
    Free,
    Scoped,
    Provider,
}

/// Sort order for the model list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    Default,
    CostAsc,
    CostDesc,
}

/// One provider tab in the model picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProviderTab {
    pub id: String,
    pub label: String,
    pub model_count: usize,
}

/// Catalog row with stable selection value (`provider/model_id`).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRow {
    pub value: String,
    pub name: String,
    pub provider_id: String,
    pub model_id: String,
    pub context_k: u32,
    pub reasoning: bool,
    pub images: bool,
    pub is_free: bool,
    pub cost_per_m_input: f64,
}

fn model_row_from_builtin(provider_id: &str, model: &Model) -> ModelRow {
    let is_free = (model.cost.input == 0.0 && model.cost.output == 0.0) || model.id.contains("free");
    ModelRow {
        value: format!("{provider_id}/{}", model.id),
        name: model.name.clone(),
        provider_id: provider_id.to_string(),
        model_id: model.id.clone(),
        context_k: model.context_window / 1000,
        reasoning: model.reasoning,
        images: model.input.iter().any(|cap| cap == "image"),
        is_free,
        cost_per_m_input: model.cost.input,
    }
}

/// Compact context size for list rows (`128K`, `1M`).
pub fn format_model_context_label(context_k: u32) -> String {
    if context_k >= 1000 && context_k.is_multiple_of(1000) {
        format!("{}M", context_k / 1000)
    } else {
        format!("{context_k}K")
    }
}

/// Capability badges for list rows: `(think)`, `(img)`, or `(think|img)`.
pub fn format_model_capability_label(reasoning: bool, images: bool) -> Option<String> {
    let mut caps = Vec::new();
    if reasoning {
        caps.push("think");
    }
    if images {
        caps.push("img");
    }
    if caps.is_empty() {
        None
    } else {
        Some(format!("({})", caps.join("|")))
    }
}

/// Options for building the model picker catalog.
#[derive(Debug, Clone, Default)]
pub struct ModelCatalogOptions {
    /// When true, only providers with stored credentials appear in All / Provider tabs.
    pub show_configured_only: bool,
    /// Always include these provider ids even if unconfigured (e.g. the active session provider).
    pub include_provider_ids: Vec<String>,
}

impl ModelCatalogOptions {
    /// Full builtin catalog (no auth filter). Used by unit tests and diagnostics.
    pub fn unfiltered() -> Self {
        Self {
            show_configured_only: false,
            include_provider_ids: Vec::new(),
        }
    }
}

/// Provider ids that already have credentials in `auth.json`.
pub fn configured_provider_ids() -> HashSet<String> {
    // Use paths resolve to find auth.json and read directly from file.
    // Cannot use get_provider_options() because its config_status is cached as Unconfigured.
    if let Ok(paths) = crate::platform::Paths::resolve() {
        let ids = crate::tui::provider_credential_store::list_providers_with_credentials(&paths.auth_store_path());
        ids.into_iter().collect()
    } else {
        HashSet::new()
    }
}

/// Built-in model catalog for the picker UI.
#[derive(Debug, Clone)]
pub struct ModelCatalogSnapshot {
    pub providers: Vec<ModelProviderTab>,
    pub models_by_provider: HashMap<String, Vec<ModelRow>>,
    /// Every model across providers (tab [`ALL_PROVIDERS_TAB_INDEX`]).
    pub all_models: Vec<ModelRow>,
    pub total_providers: usize,
    pub total_models: usize,
    /// Whether this snapshot was built with the configured-only filter.
    pub show_configured_only: bool,
}

impl ModelCatalogSnapshot {
    /// Full unfiltered catalog (tests / callers that want every builtin provider).
    #[cfg(test)]
    pub fn build(scoped_model_items: &[String]) -> Self {
        Self::build_with_options(scoped_model_items, &ModelCatalogOptions::unfiltered())
    }

    pub fn build_with_options(scoped_model_items: &[String], options: &ModelCatalogOptions) -> Self {
        // Don't clear cache here - cache key includes models.len() so stale entries won't match
        // Cache is only cleared when explicitly needed (e.g., manual refresh)
        let allowed: Option<HashSet<String>> = if options.show_configured_only {
            let mut set = configured_provider_ids();
            for id in &options.include_provider_ids {
                if !id.is_empty() {
                    set.insert(id.clone());
                }
            }
            Some(set)
        } else {
            None
        };

        let provider_ids = get_builtin_providers();
        let mut providers = Vec::new();
        let mut models_by_provider = HashMap::new();
        let mut total_models = 0usize;

        let mut all_models: Vec<ModelRow> = Vec::new();
        for provider_id in &provider_ids {
            if let Some(ref allowed) = allowed
                && !allowed.contains(provider_id.as_str())
            {
                continue;
            }
            let models = get_builtin_models(provider_id);
            let count = models.len();
            total_models = total_models.saturating_add(count);
            let rows: Vec<ModelRow> = models
                .iter()
                .map(|model| model_row_from_builtin(provider_id, model))
                .collect();
            all_models.extend(rows.iter().cloned());
            providers.push(ModelProviderTab {
                id: provider_id.clone(),
                label: format_provider_label(provider_id),
                model_count: count,
            });
            models_by_provider.insert(provider_id.clone(), rows);
        }

        let total_providers = providers.len();
        all_models.sort_by(|left, right| left.name.cmp(&right.name).then_with(|| left.value.cmp(&right.value)));

        let scoped_models = build_scoped_model_rows(scoped_model_items);
        let scoped_count = scoped_models.len();

        // Build free models from all_models
        let free_models: Vec<ModelRow> = all_models.iter().filter(|row| row.is_free).cloned().collect();
        let free_count = free_models.len();

        providers.insert(
            0,
            ModelProviderTab {
                id: ALL_PROVIDERS_TAB_ID.to_string(),
                label: ALL_PROVIDERS_TAB_LABEL.to_string(),
                model_count: total_models,
            },
        );
        providers.insert(
            1,
            ModelProviderTab {
                id: FREE_PROVIDERS_TAB_ID.to_string(),
                label: FREE_PROVIDERS_TAB_LABEL.to_string(),
                model_count: free_count,
            },
        );
        providers.insert(
            2,
            ModelProviderTab {
                id: SCOPED_PROVIDERS_TAB_ID.to_string(),
                label: SCOPED_PROVIDERS_TAB_LABEL.to_string(),
                model_count: scoped_count,
            },
        );
        models_by_provider.insert(ALL_PROVIDERS_TAB_ID.to_string(), all_models.clone());
        models_by_provider.insert(FREE_PROVIDERS_TAB_ID.to_string(), free_models);
        // Scoped tab models live only in the map (same pattern as per-provider lists).
        models_by_provider.insert(SCOPED_PROVIDERS_TAB_ID.to_string(), scoped_models);

        Self {
            providers,
            models_by_provider,
            all_models,
            total_providers,
            total_models,
            show_configured_only: options.show_configured_only,
        }
    }

    pub fn provider_tab_count(&self) -> usize {
        self.providers.len()
    }

    pub fn provider_id(&self, index: usize) -> Option<&str> {
        self.providers.get(index).map(|tab| tab.id.as_str())
    }

    pub fn is_all_providers_tab(&self, index: usize) -> bool {
        self.provider_id(index) == Some(ALL_PROVIDERS_TAB_ID)
    }

    pub fn is_free_providers_tab(&self, index: usize) -> bool {
        self.provider_id(index) == Some(FREE_PROVIDERS_TAB_ID)
    }

    pub fn is_scoped_providers_tab(&self, index: usize) -> bool {
        self.provider_id(index) == Some(SCOPED_PROVIDERS_TAB_ID)
    }

    pub fn scope_mode(&self, provider_index: usize) -> ModelScopeMode {
        if self.is_all_providers_tab(provider_index) {
            ModelScopeMode::All
        } else if self.is_free_providers_tab(provider_index) {
            ModelScopeMode::Free
        } else if self.is_scoped_providers_tab(provider_index) {
            ModelScopeMode::Scoped
        } else {
            ModelScopeMode::Provider
        }
    }

    pub fn builtin_provider_indices(&self) -> Vec<usize> {
        self.providers
            .iter()
            .enumerate()
            .filter(|(_, tab)| !is_synthetic_provider_tab(&tab.id))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn first_builtin_provider_index(&self) -> usize {
        self.builtin_provider_indices()
            .first()
            .copied()
            .unwrap_or(BUILTIN_PROVIDERS_START_INDEX)
    }
}

pub fn scope_tab_index(mode: ModelScopeMode) -> usize {
    match mode {
        ModelScopeMode::All => ALL_PROVIDERS_TAB_INDEX,
        ModelScopeMode::Free => FREE_PROVIDERS_TAB_INDEX,
        ModelScopeMode::Scoped => SCOPED_PROVIDERS_TAB_INDEX,
        ModelScopeMode::Provider => PROVIDER_SCOPE_TAB_INDEX,
    }
}

pub fn scope_mode_from_tab_index(index: usize) -> ModelScopeMode {
    match index {
        ALL_PROVIDERS_TAB_INDEX => ModelScopeMode::All,
        FREE_PROVIDERS_TAB_INDEX => ModelScopeMode::Free,
        SCOPED_PROVIDERS_TAB_INDEX => ModelScopeMode::Scoped,
        _ => ModelScopeMode::Provider,
    }
}

pub fn scope_tab_labels() -> [&'static str; SCOPE_TAB_COUNT] {
    [
        ALL_PROVIDERS_TAB_LABEL,
        FREE_PROVIDERS_TAB_LABEL,
        SCOPED_PROVIDERS_TAB_LABEL,
        PROVIDER_SCOPE_TAB_LABEL,
    ]
}

pub fn is_synthetic_provider_tab(id: &str) -> bool {
    id == ALL_PROVIDERS_TAB_ID || id == FREE_PROVIDERS_TAB_ID || id == SCOPED_PROVIDERS_TAB_ID
}

pub fn build_scoped_model_rows(scoped_model_items: &[String]) -> Vec<ModelRow> {
    let mut rows = Vec::new();
    for item in scoped_model_items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok((provider_id, model_id)) = parse_model_value(trimmed) else {
            continue;
        };
        let Some(model) = get_builtin_model(&provider_id, &model_id) else {
            continue;
        };
        let value = format!("{provider_id}/{}", model.id);
        if rows.iter().any(|row: &ModelRow| row.value == value) {
            continue;
        }
        rows.push(model_row_from_builtin(&provider_id, &model));
    }
    rows
}

/// Keyboard focus within the model picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelSelectorFocus {
    #[default]
    Search,
    List,
}

/// Open model picker session.
#[derive(Debug, Clone)]
pub struct PendingModelSelector {
    pub catalog: ModelCatalogSnapshot,
    pub provider_index: usize,
    /// Last built-in provider tab when switching between scope modes.
    pub last_builtin_provider_index: usize,
    pub model_index: usize,
    pub filter: String,
    pub stashed_prompt_draft: Option<String>,
    pub input_focus: ModelSelectorFocus,
    pub sort_order: SortOrder,
}

impl PendingModelSelector {
    #[cfg(test)]
    pub fn open(initial_filter: String, stashed_prompt_draft: Option<String>, scoped_model_items: &[String]) -> Self {
        Self::open_with_options(
            initial_filter,
            stashed_prompt_draft,
            scoped_model_items,
            &ModelCatalogOptions::unfiltered(),
        )
    }

    pub fn open_with_options(
        initial_filter: String,
        stashed_prompt_draft: Option<String>,
        scoped_model_items: &[String],
        catalog_options: &ModelCatalogOptions,
    ) -> Self {
        let catalog = ModelCatalogSnapshot::build_with_options(scoped_model_items, catalog_options);
        let last_builtin_provider_index = catalog.first_builtin_provider_index();
        Self {
            catalog,
            provider_index: ALL_PROVIDERS_TAB_INDEX,
            last_builtin_provider_index,
            model_index: 0,
            filter: initial_filter,
            stashed_prompt_draft,
            input_focus: ModelSelectorFocus::Search,
            sort_order: SortOrder::Default,
        }
    }

    /// Open on the **All** tab (default), highlighting the current model when known.
    ///
    /// Remembers the built-in provider for later Provider-tab navigation; does not land on
    /// Scoped/Provider unless the user switches with `[` / `]`.
    #[cfg(test)]
    pub fn open_with_selection(
        initial_filter: String,
        stashed_prompt_draft: Option<String>,
        scoped_model_items: &[String],
        provider_id: Option<&str>,
        model_id: Option<&str>,
    ) -> Self {
        Self::open_with_selection_options(
            initial_filter,
            stashed_prompt_draft,
            scoped_model_items,
            provider_id,
            model_id,
            &ModelCatalogOptions::unfiltered(),
        )
    }

    pub fn open_with_selection_options(
        initial_filter: String,
        stashed_prompt_draft: Option<String>,
        scoped_model_items: &[String],
        provider_id: Option<&str>,
        model_id: Option<&str>,
        catalog_options: &ModelCatalogOptions,
    ) -> Self {
        let mut options = catalog_options.clone();
        if let Some(provider) = provider_id {
            options.include_provider_ids.push(provider.to_string());
        }
        let mut pending = Self::open_with_options(initial_filter, stashed_prompt_draft, scoped_model_items, &options);
        // Always start on All.
        pending.provider_index = ALL_PROVIDERS_TAB_INDEX;
        if let (Some(provider), Some(model)) = (provider_id, model_id) {
            let value = format!("{provider}/{model}");
            // Remember the real built-in provider for Provider-tab restore.
            if let Some(builtin_pi) = pending.catalog.providers.iter().position(|tab| tab.id == provider) {
                pending.last_builtin_provider_index = builtin_pi;
            }
            // Highlight the current model within the All list when present.
            let models = pending.filtered_models();
            if let Some(mi) = models.iter().position(|row| row.value == value) {
                pending.model_index = mi;
            }
        }
        pending.clamp_indices();
        pending
    }

    /// Built-in provider catalog index used when entering Provider scope mode.
    fn resolved_builtin_provider_index(&self) -> usize {
        if matches!(
            self.catalog.scope_mode(self.last_builtin_provider_index),
            ModelScopeMode::Provider
        ) {
            self.last_builtin_provider_index
        } else {
            self.catalog.first_builtin_provider_index()
        }
    }

    pub fn scope_mode(&self) -> ModelScopeMode {
        self.catalog.scope_mode(self.provider_index)
    }

    pub fn is_provider_scope_mode(&self) -> bool {
        matches!(self.scope_mode(), ModelScopeMode::Provider)
    }

    pub fn active_provider_id(&self) -> Option<&str> {
        self.catalog.provider_id(self.provider_index)
    }

    /// Rebuild the Scoped tab from the live session scoped list.
    pub fn refresh_scoped_models(&mut self, scoped_model_items: &[String]) {
        self.catalog.refresh_scoped_models(scoped_model_items);
        self.clamp_indices();
    }

    pub fn filtered_models(&self) -> Vec<ModelRow> {
        // Fuzzy search (and the empty-filter list) stay inside the active tab's category:
        // All → every model, Free → free only, Scoped → scoped only, Provider → that provider.
        let provider = match self.active_provider_id() {
            Some(id) => id,
            None => return Vec::new(),
        };
        let mut models = if provider == ALL_PROVIDERS_TAB_ID {
            self.catalog.all_models.clone()
        } else {
            self.catalog
                .models_by_provider
                .get(provider)
                .cloned()
                .unwrap_or_default()
        };
        if !self.filter.trim().is_empty() {
            models = filter_models_fuzzy(&models, &self.filter);
        }
        Self::apply_sort(&mut models, self.sort_order);
        models
    }

    /// Apply sort order to a model list in place.
    fn apply_sort(models: &mut [ModelRow], order: SortOrder) {
        match order {
            SortOrder::Default => {
                // Keep existing order (grouped by provider, then name)
            }
            SortOrder::CostAsc => {
                models.sort_by(|a, b| {
                    a.cost_per_m_input
                        .partial_cmp(&b.cost_per_m_input)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.model_id.cmp(&b.model_id))
                });
            }
            SortOrder::CostDesc => {
                models.sort_by(|a, b| {
                    b.cost_per_m_input
                        .partial_cmp(&a.cost_per_m_input)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.model_id.cmp(&b.model_id))
                });
            }
        }
    }

    pub fn selected_model(&self) -> Option<ModelRow> {
        self.filtered_models().get(self.model_index).cloned()
    }

    pub fn clamp_indices(&mut self) {
        let tab_len = self.catalog.provider_tab_count();
        if tab_len == 0 {
            self.provider_index = ALL_PROVIDERS_TAB_INDEX;
            self.model_index = 0;
            return;
        }
        self.provider_index = self.provider_index.min(tab_len.saturating_sub(1));
        let model_len = self.filtered_models().len();
        if model_len == 0 {
            self.model_index = 0;
        } else {
            self.model_index = self.model_index.min(model_len - 1);
        }
    }

    pub fn set_provider_index(&mut self, index: usize) {
        self.provider_index = index;
        self.model_index = 0;
        self.clamp_indices();
    }

    pub fn set_scope_mode(&mut self, mode: ModelScopeMode) {
        if self.is_provider_scope_mode() {
            self.last_builtin_provider_index = self.provider_index;
        }
        let provider_target = self.resolved_builtin_provider_index();
        self.provider_index = match mode {
            ModelScopeMode::All => ALL_PROVIDERS_TAB_INDEX,
            ModelScopeMode::Free => FREE_PROVIDERS_TAB_INDEX,
            ModelScopeMode::Scoped => SCOPED_PROVIDERS_TAB_INDEX,
            ModelScopeMode::Provider => provider_target,
        };
        if matches!(mode, ModelScopeMode::Provider) {
            self.last_builtin_provider_index = provider_target;
        }
        self.model_index = 0;
        self.clamp_indices();
    }

    pub fn scope_nav_delta(&self, delta: isize) -> ModelScopeMode {
        let current = scope_tab_index(self.scope_mode());
        let next = (current as isize + delta).rem_euclid(SCOPE_TAB_COUNT as isize) as usize;
        scope_mode_from_tab_index(next)
    }

    pub fn apply_scope_nav(&mut self, delta: isize) {
        let next_mode = self.scope_nav_delta(delta);
        self.set_scope_mode(next_mode);
    }

    /// `←/→` cycles built-in providers **only while the Provider scope tab is active**.
    ///
    /// On All / Scoped, arrows are ignored — use `[` / `]` to switch scope tabs first.
    pub fn apply_provider_nav(&mut self, delta: isize) {
        if !self.is_provider_scope_mode() {
            return;
        }
        let indices = self.catalog.builtin_provider_indices();
        if indices.is_empty() {
            return;
        }

        let current_pos = indices
            .iter()
            .position(|&index| index == self.provider_index)
            .unwrap_or(0);
        let next = (current_pos as isize + delta).rem_euclid(indices.len() as isize) as usize;
        let target = indices[next];
        self.last_builtin_provider_index = target;
        self.set_provider_index(target);
    }
}

pub fn format_provider_label(provider_id: &str) -> String {
    provider_id
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rows reserved in the picker body above the scrollable model list.
pub const MODEL_SELECTOR_LIST_FIXED_ROWS: u16 = 4;

/// Fixed scroll viewport height — stable when switching providers or filters.
pub fn model_selector_list_viewport_height(screen_width: u16, screen_height: u16) -> u16 {
    let theme = UiTheme::default();
    let chrome = DialogChrome::from_theme(theme, screen_width);
    let max_body = dialog_max_content_height(screen_height, &chrome, 12);
    (list_viewport_cap(screen_height).min(max_body.saturating_sub(MODEL_SELECTOR_LIST_FIXED_ROWS) as usize) as u16)
        .max(4)
}

pub fn global_count_label(catalog: &ModelCatalogSnapshot) -> String {
    if catalog.show_configured_only {
        format!("{} configured · {} models", catalog.total_providers, catalog.total_models)
    } else {
        format!(
            "{} providers · {} models available",
            catalog.total_providers, catalog.total_models
        )
    }
}

pub fn model_selector_footer_hint(in_provider_scope: bool, sort_order: SortOrder) -> String {
    let sort_hint = match sort_order {
        SortOrder::Default => "",
        SortOrder::CostAsc => " · $ asc",
        SortOrder::CostDesc => " · $ desc",
    };
    let base = if in_provider_scope {
        "↑/↓ model · ←/→ provider · [ ] scope · + add scoped · − remove · / filter · Enter confirm · Esc".to_string()
    } else {
        "↑/↓ model · [ ] scope · + add scoped · − remove · / filter · Enter confirm · Esc".to_string()
    };
    format!("{}{} · $ sort", base, sort_hint)
}

/// Refresh the Scoped tab after the live scoped list changes (add/remove from picker).
impl ModelCatalogSnapshot {
    pub fn refresh_scoped_models(&mut self, scoped_model_items: &[String]) {
        let scoped_models = build_scoped_model_rows(scoped_model_items);
        let scoped_count = scoped_models.len();
        if let Some(tab) = self.providers.iter_mut().find(|tab| tab.id == SCOPED_PROVIDERS_TAB_ID) {
            tab.model_count = scoped_count;
        }
        self.models_by_provider
            .insert(SCOPED_PROVIDERS_TAB_ID.to_string(), scoped_models);
    }
}

pub fn model_match_score(row: &ModelRow, query: &str) -> Option<i32> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Some(0);
    }

    let provider_label = format_provider_label(&row.provider_id).to_ascii_lowercase();
    let context_label = format_model_context_label(row.context_k).to_ascii_lowercase();

    let mut best = field_score(&query, &row.name.to_ascii_lowercase(), NAME_WEIGHT, true);
    best = max_score(best, field_score(&query, &row.model_id.to_ascii_lowercase(), ID_WEIGHT, true));
    best = max_score(
        best,
        field_score(&query, &row.provider_id.to_ascii_lowercase(), ID_WEIGHT, true),
    );
    best = max_score(best, field_score(&query, &provider_label, ID_WEIGHT, true));
    best = max_score(best, field_score(&query, &context_label, DESCRIPTION_WEIGHT, true));
    if row.reasoning {
        best = max_score(best, field_score(&query, "think reasoning", DESCRIPTION_WEIGHT, false));
    }
    if row.images {
        best = max_score(best, field_score(&query, "img image vision", DESCRIPTION_WEIGHT, false));
    }
    best
}

fn model_query_tokens(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn model_row_match_score(row: &ModelRow, query: &str) -> Option<i32> {
    let tokens = model_query_tokens(query);
    if tokens.is_empty() {
        return Some(0);
    }
    if tokens.len() == 1 {
        return model_match_score(row, &tokens[0]);
    }

    let mut total = 0i32;
    for token in &tokens {
        total = total.saturating_add(model_match_score(row, token)?);
    }
    Some(total)
}

pub fn filter_models_fuzzy(models: &[ModelRow], query: &str) -> Vec<ModelRow> {
    let query = query.trim();
    if query.is_empty() {
        return models.to_vec();
    }

    let mut scored: Vec<(&ModelRow, i32)> = models
        .iter()
        .filter_map(|row| model_row_match_score(row, query).map(|score| (row, score)))
        .collect();
    // Group by provider first so search results stay clustered; within a provider
    // keep higher fuzzy scores (then name) for scanability.
    scored.sort_by(|left, right| {
        left.0
            .provider_id
            .cmp(&right.0.provider_id)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.0.name.cmp(&right.0.name))
            .then_with(|| left.0.model_id.cmp(&right.0.model_id))
    });
    scored.into_iter().map(|(row, _)| row.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_explicit_model_selection(provider_id: Option<&str>, model_id: Option<&str>) -> bool {
        provider_id.is_some() && model_id.is_some()
    }

    #[test]
    fn format_provider_label_title_cases_hyphens() {
        assert_eq!(format_provider_label("amazon-bedrock"), "Amazon Bedrock");
        assert_eq!(format_provider_label("anthropic"), "Anthropic");
    }

    #[test]
    fn filter_with_query_searches_global_on_non_provider_tab() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        // When on All tab, fuzzy search searches all models globally
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
        pending.filter = "gpt-5.6-luna".to_string();
        let filtered = pending.filtered_models();
        assert!(
            filtered.iter().any(|row| row.model_id == "gpt-5.6-luna"),
            "expected global fuzzy search to find gpt-5.6-luna from All tab"
        );
    }

    #[test]
    fn filter_with_query_restricts_to_provider_on_provider_tab() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        let anthropic = pending
            .catalog
            .providers
            .iter()
            .position(|tab| tab.id == "anthropic")
            .expect("anthropic provider");
        pending.set_provider_index(anthropic);
        assert!(pending.is_provider_scope_mode());
        pending.filter = "gpt-5.6-luna".to_string();
        let filtered = pending.filtered_models();
        assert!(
            filtered.iter().all(|row| row.provider_id == "anthropic"),
            "expected fuzzy search to be restricted to anthropic on provider tab"
        );
    }

    #[test]
    fn filter_on_free_tab_only_searches_free_models() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.set_provider_index(FREE_PROVIDERS_TAB_INDEX);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Free);

        // A paid model must never surface through the Free tab filter.
        if let Some(paid) = pending.catalog.all_models.iter().find(|row| !row.is_free) {
            pending.filter = paid.model_id.clone();
            let filtered = pending.filtered_models();
            for row in &filtered {
                assert!(row.is_free, "paid model leaked into Free tab filter: {}", row.value);
            }
        }

        // A known free model is still findable on the Free tab.
        if let Some(free_id) = pending
            .catalog
            .all_models
            .iter()
            .find(|row| row.is_free)
            .map(|row| row.model_id.clone())
        {
            pending.filter = free_id;
            let filtered = pending.filtered_models();
            assert!(!filtered.is_empty());
            assert!(filtered.iter().all(|row| row.is_free));
        }
    }

    #[test]
    fn filter_on_scoped_tab_only_searches_scoped_models() {
        let base = ModelCatalogSnapshot::build(&[]);
        let sample = base.all_models.first().expect("model").value.clone();
        let mut pending = PendingModelSelector::open(String::new(), None, std::slice::from_ref(&sample));
        pending.set_provider_index(SCOPED_PROVIDERS_TAB_INDEX);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);

        let sample_id = sample.split('/').nth(1).expect("model id");
        pending.filter = sample_id.to_string();
        let filtered = pending.filtered_models();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, sample);

        // A query matching a non-scoped model id must not pull it in.
        if let Some(other) = base.all_models.iter().find(|row| row.value != sample) {
            pending.filter = other.model_id.clone();
            let filtered = pending.filtered_models();
            assert!(
                filtered.iter().all(|row| row.value == sample),
                "scoped tab filter leaked non-scoped models: {:?}",
                filtered.iter().map(|row| row.value.as_str()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn fuzzy_filter_matches_formatted_provider_label() {
        let rows = vec![ModelRow {
            value: "amazon-bedrock/claude-sonnet-4".into(),
            name: "Claude Sonnet 4".into(),
            provider_id: "amazon-bedrock".into(),
            model_id: "claude-sonnet-4".into(),
            context_k: 200,
            reasoning: false,
            images: false,
            is_free: false,
            cost_per_m_input: 0.0,
        }];
        let filtered = filter_models_fuzzy(&rows, "bedrock");
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn fuzzy_filter_requires_every_token_for_multi_word_queries() {
        let rows = vec![
            ModelRow {
                value: "anthropic/claude-sonnet-4".into(),
                name: "Claude Sonnet 4".into(),
                provider_id: "anthropic".into(),
                model_id: "claude-sonnet-4".into(),
                context_k: 200,
                reasoning: false,
                images: false,
                is_free: false,
                cost_per_m_input: 0.0,
            },
            ModelRow {
                value: "anthropic/claude-opus-4".into(),
                name: "Claude Opus 4".into(),
                provider_id: "anthropic".into(),
                model_id: "claude-opus-4".into(),
                context_k: 200,
                reasoning: true,
                images: true,
                is_free: false,
                cost_per_m_input: 0.0,
            },
        ];
        let filtered = filter_models_fuzzy(&rows, "sonnet 4");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Claude Sonnet 4");
    }

    #[test]
    fn opus_token_does_not_false_positive_on_sonnet() {
        let sonnet = ModelRow {
            value: "anthropic/claude-sonnet-4".into(),
            name: "Claude Sonnet 4".into(),
            provider_id: "anthropic".into(),
            model_id: "claude-sonnet-4".into(),
            context_k: 200,
            reasoning: false,
            images: false,
            is_free: false,
            cost_per_m_input: 0.0,
        };
        assert_eq!(model_match_score(&sonnet, "opus"), None);
        assert_eq!(model_row_match_score(&sonnet, "opus 4"), None);
    }

    #[test]
    fn fuzzy_filter_matches_model_name_subsequence() {
        let rows = vec![
            ModelRow {
                value: "anthropic/claude-sonnet-4".into(),
                name: "Claude Sonnet 4".into(),
                provider_id: "anthropic".into(),
                model_id: "claude-sonnet-4".into(),
                context_k: 200,
                reasoning: false,
                images: false,
                is_free: false,
                cost_per_m_input: 0.0,
            },
            ModelRow {
                value: "anthropic/claude-opus-4".into(),
                name: "Claude Opus 4".into(),
                provider_id: "anthropic".into(),
                model_id: "claude-opus-4".into(),
                context_k: 200,
                reasoning: true,
                images: true,
                is_free: false,
                cost_per_m_input: 0.0,
            },
        ];
        let filtered = filter_models_fuzzy(&rows, "opus 4");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Claude Opus 4");
    }

    #[test]
    fn empty_filter_returns_all_models_in_order() {
        let rows = vec![
            ModelRow {
                value: "a/m1".into(),
                name: "M1".into(),
                provider_id: "a".into(),
                model_id: "m1".into(),
                context_k: 128,
                reasoning: false,
                images: false,
                is_free: false,
                cost_per_m_input: 0.0,
            },
            ModelRow {
                value: "a/m2".into(),
                name: "M2".into(),
                provider_id: "a".into(),
                model_id: "m2".into(),
                context_k: 128,
                reasoning: false,
                images: false,
                is_free: false,
                cost_per_m_input: 0.0,
            },
        ];
        assert_eq!(filter_models_fuzzy(&rows, ""), rows);
    }

    #[test]
    fn search_results_are_grouped_by_provider() {
        let rows = vec![
            ModelRow {
                value: "zeta/claude-a".into(),
                name: "Claude A".into(),
                provider_id: "zeta".into(),
                model_id: "claude-a".into(),
                context_k: 100,
                reasoning: false,
                images: false,
                is_free: false,
                cost_per_m_input: 0.0,
            },
            ModelRow {
                value: "alpha/claude-b".into(),
                name: "Claude B".into(),
                provider_id: "alpha".into(),
                model_id: "claude-b".into(),
                context_k: 100,
                reasoning: false,
                images: false,
                is_free: false,
                cost_per_m_input: 0.0,
            },
            ModelRow {
                value: "zeta/claude-c".into(),
                name: "Claude C".into(),
                provider_id: "zeta".into(),
                model_id: "claude-c".into(),
                context_k: 100,
                reasoning: false,
                images: false,
                is_free: false,
                cost_per_m_input: 0.0,
            },
            ModelRow {
                value: "alpha/claude-d".into(),
                name: "Claude D".into(),
                provider_id: "alpha".into(),
                model_id: "claude-d".into(),
                context_k: 100,
                reasoning: false,
                images: false,
                is_free: false,
                cost_per_m_input: 0.0,
            },
        ];
        let filtered = filter_models_fuzzy(&rows, "claude");
        assert_eq!(filtered.len(), 4);
        let providers: Vec<&str> = filtered.iter().map(|r| r.provider_id.as_str()).collect();
        // All alpha rows appear before zeta (alphabetical group), not interleaved.
        assert_eq!(providers, vec!["alpha", "alpha", "zeta", "zeta"]);
    }

    #[test]
    fn context_label_uses_k_and_m_suffixes() {
        assert_eq!(format_model_context_label(128), "128K");
        assert_eq!(format_model_context_label(1000), "1M");
        assert_eq!(format_model_context_label(2000), "2M");
        assert_eq!(format_model_context_label(1048), "1048K");
    }

    #[test]
    fn capability_label_joins_think_and_img() {
        assert_eq!(format_model_capability_label(true, false).as_deref(), Some("(think)"));
        assert_eq!(format_model_capability_label(false, true).as_deref(), Some("(img)"));
        assert_eq!(format_model_capability_label(true, true).as_deref(), Some("(think|img)"));
        assert_eq!(format_model_capability_label(false, false), None);
    }

    #[test]
    fn list_viewport_height_is_stable_across_screen_sizes() {
        let tall = model_selector_list_viewport_height(120, 40);
        assert_eq!(tall, 8);
        assert_eq!(tall, model_selector_list_viewport_height(120, 40));

        let medium = model_selector_list_viewport_height(120, 30);
        assert_eq!(medium, 6);

        let short = model_selector_list_viewport_height(120, 20);
        assert_eq!(short, 4);
    }

    #[test]
    fn global_count_label_formats_totals() {
        let catalog = ModelCatalogSnapshot {
            providers: vec![],
            models_by_provider: HashMap::new(),
            all_models: vec![],
            total_providers: 3,
            total_models: 12,
            show_configured_only: false,
        };
        assert_eq!(global_count_label(&catalog), "3 providers · 12 models available");
        let configured = ModelCatalogSnapshot {
            show_configured_only: true,
            ..catalog
        };
        assert_eq!(global_count_label(&configured), "3 configured · 12 models");
    }

    #[test]
    fn has_explicit_model_selection_requires_both_fields() {
        assert!(!has_explicit_model_selection(None, None));
        assert!(!has_explicit_model_selection(Some("anthropic"), None));
        assert!(!has_explicit_model_selection(None, Some("claude")));
        assert!(has_explicit_model_selection(Some("anthropic"), Some("claude")));
    }

    #[test]
    fn catalog_builds_nonempty_snapshot() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        assert!(catalog.total_providers > 0);
        assert!(catalog.total_models > 0);
        assert!(
            catalog
                .providers
                .iter()
                .filter(|tab| !is_synthetic_provider_tab(&tab.id))
                .all(|tab| tab.model_count > 0)
        );
    }

    #[test]
    fn catalog_places_all_tab_first_with_every_model() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let all_tab = catalog.providers.first().expect("providers");
        assert_eq!(all_tab.id, ALL_PROVIDERS_TAB_ID);
        assert_eq!(all_tab.label, ALL_PROVIDERS_TAB_LABEL);
        assert_eq!(all_tab.model_count, catalog.total_models);
        assert_eq!(catalog.all_models.len(), catalog.total_models);
        assert_eq!(
            catalog
                .models_by_provider
                .get(ALL_PROVIDERS_TAB_ID)
                .map(Vec::len)
                .unwrap_or(0),
            catalog.total_models
        );
    }

    #[test]
    fn catalog_places_free_tab_after_all_and_before_scoped() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        assert_eq!(catalog.providers[0].id, ALL_PROVIDERS_TAB_ID);
        assert_eq!(catalog.providers[1].id, FREE_PROVIDERS_TAB_ID);
        assert_eq!(catalog.providers[1].label, FREE_PROVIDERS_TAB_LABEL);
        assert_eq!(catalog.providers[2].id, SCOPED_PROVIDERS_TAB_ID);
    }

    #[test]
    fn catalog_free_tab_contains_exactly_free_models() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let free = catalog
            .models_by_provider
            .get(FREE_PROVIDERS_TAB_ID)
            .expect("free models");
        for row in free {
            assert!(row.is_free, "non-free model in Free tab: {}", row.value);
        }
        assert_eq!(catalog.providers[1].model_count, free.len());
    }

    #[test]
    fn scoped_tab_lists_configured_models_in_order() {
        let base = ModelCatalogSnapshot::build(&[]);
        let sample = base.all_models.first().expect("model").value.clone();
        let catalog = ModelCatalogSnapshot::build(std::slice::from_ref(&sample));
        assert_eq!(catalog.providers[2].model_count, 1);
        let scoped = catalog
            .models_by_provider
            .get(SCOPED_PROVIDERS_TAB_ID)
            .expect("scoped models");
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].value, sample);
    }

    #[test]
    fn build_scoped_model_rows_skips_unknown_entries() {
        let base = ModelCatalogSnapshot::build(&[]);
        let sample = base.all_models.first().expect("model").value.clone();
        let rows = build_scoped_model_rows(&["not-a-model".into(), sample.clone(), sample.clone()]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, sample);
    }

    #[test]
    fn open_with_selection_defaults_to_all_tab() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let provider_id = catalog
            .providers
            .iter()
            .find(|tab| !is_synthetic_provider_tab(&tab.id))
            .map(|tab| tab.id.as_str())
            .expect("builtin provider");
        let model_id = catalog
            .models_by_provider
            .get(provider_id)
            .and_then(|rows| rows.first())
            .map(|row| row.value.split('/').nth(1).expect("model id"))
            .expect("provider model");

        let pending =
            PendingModelSelector::open_with_selection(String::new(), None, &[], Some(provider_id), Some(model_id));
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
        assert!(catalog.is_all_providers_tab(pending.provider_index));
        assert_eq!(
            pending.selected_model().map(|row| row.value),
            Some(format!("{provider_id}/{model_id}"))
        );
        // Provider restore still points at the model's built-in tab.
        let builtin = catalog
            .providers
            .iter()
            .position(|tab| tab.id == provider_id)
            .expect("builtin provider tab");
        assert_eq!(pending.last_builtin_provider_index, builtin);
    }

    #[test]
    fn open_with_selection_stays_on_all_even_when_model_is_scoped() {
        let base = ModelCatalogSnapshot::build(&[]);
        let sample = base.all_models.first().expect("model");
        let (provider_id, model_id) = sample.value.split_once('/').expect("provider/model");
        let pending = PendingModelSelector::open_with_selection(
            String::new(),
            None,
            std::slice::from_ref(&sample.value),
            Some(provider_id),
            Some(model_id),
        );
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
        assert_eq!(pending.selected_model().map(|row| row.value), Some(sample.value.clone()));
        assert!(
            matches!(
                pending.catalog.scope_mode(pending.last_builtin_provider_index),
                ModelScopeMode::Provider
            ),
            "last_builtin_provider_index must be a real provider tab, got {}",
            pending.last_builtin_provider_index
        );
    }

    #[test]
    fn scope_nav_reaches_provider_from_default_all() {
        let base = ModelCatalogSnapshot::build(&[]);
        let sample = base.all_models.first().expect("model");
        let (provider_id, model_id) = sample.value.split_once('/').expect("provider/model");
        let mut pending = PendingModelSelector::open_with_selection(
            String::new(),
            None,
            std::slice::from_ref(&sample.value),
            Some(provider_id),
            Some(model_id),
        );
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
        // ] from All → Free, ] → Scoped, ] → Provider.
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Free);
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Provider);
        assert!(!pending.catalog.is_all_providers_tab(pending.provider_index));
        assert!(!pending.catalog.is_scoped_providers_tab(pending.provider_index));
    }

    #[test]
    fn set_scope_mode_provider_recovers_from_corrupt_last_builtin() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.last_builtin_provider_index = SCOPED_PROVIDERS_TAB_INDEX;
        pending.set_scope_mode(ModelScopeMode::Provider);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Provider);
        assert_eq!(pending.provider_index, pending.catalog.first_builtin_provider_index());
    }

    #[test]
    fn scope_brackets_cycle_all_free_scoped_provider() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Free);
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Provider);
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
    }

    #[test]
    fn provider_nav_cycles_builtin_providers_only() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let builtins = catalog.builtin_provider_indices();
        assert!(builtins.len() >= 2);
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.set_scope_mode(ModelScopeMode::Provider);
        let start = pending.provider_index;
        pending.apply_provider_nav(1);
        assert_ne!(pending.provider_index, start);
        assert!(pending.is_provider_scope_mode());
        assert_eq!(pending.last_builtin_provider_index, pending.provider_index);
        // Wrapping left from first stays in Provider mode (does not jump to Scoped/All).
        pending.set_provider_index(builtins[0]);
        pending.apply_provider_nav(-1);
        assert_eq!(pending.provider_index, *builtins.last().expect("last provider"));
        assert!(pending.is_provider_scope_mode());
    }

    #[test]
    fn show_configured_only_filters_providers_and_all_models() {
        // With an explicit empty allow-list, no builtin providers should appear.
        let empty_allow = ModelCatalogOptions {
            show_configured_only: true,
            // include none; configured_provider_ids() may find real auth — force empty via
            // building with show_configured_only and then asserting filter path exists.
            include_provider_ids: Vec::new(),
        };
        let filtered = ModelCatalogSnapshot::build_with_options(&[], &empty_allow);
        let full = ModelCatalogSnapshot::build(&[]);
        // Filtered catalog never has more providers than full catalog.
        assert!(filtered.total_providers <= full.total_providers);
        assert!(filtered.total_models <= full.total_models);
        // Synthetic All/Free/Scoped always present.
        assert!(filtered.is_all_providers_tab(ALL_PROVIDERS_TAB_INDEX));
        assert!(filtered.is_free_providers_tab(FREE_PROVIDERS_TAB_INDEX));
        assert!(filtered.is_scoped_providers_tab(SCOPED_PROVIDERS_TAB_INDEX));
        // Every listed builtin provider is either configured or in include list.
        if empty_allow.show_configured_only {
            let allowed = configured_provider_ids();
            for tab in &filtered.providers {
                if is_synthetic_provider_tab(&tab.id) {
                    continue;
                }
                assert!(
                    allowed.contains(&tab.id),
                    "unconfigured provider leaked into catalog: {}",
                    tab.id
                );
            }
            for row in &filtered.all_models {
                assert!(allowed.contains(&row.provider_id), "unconfigured model leaked: {}", row.value);
            }
        }
    }

    #[test]
    fn include_provider_ids_keep_active_provider_when_unconfigured() {
        let full = ModelCatalogSnapshot::build(&[]);
        let Some(sample_id) = full
            .providers
            .iter()
            .find(|t| !is_synthetic_provider_tab(&t.id))
            .map(|t| t.id.clone())
        else {
            return;
        };
        // Only include one specific provider (ignore real configured set by using
        // show_configured_only + include that forces at least this id).
        let opts = ModelCatalogOptions {
            show_configured_only: true,
            include_provider_ids: vec![sample_id.clone()],
        };
        let catalog = ModelCatalogSnapshot::build_with_options(&[], &opts);
        assert!(
            catalog.providers.iter().any(|t| t.id == sample_id),
            "included provider must appear even if unconfigured"
        );
    }

    #[test]
    fn provider_nav_ignored_outside_provider_scope() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
        let start = pending.provider_index;
        pending.apply_provider_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
        assert_eq!(pending.provider_index, start);

        pending.set_scope_mode(ModelScopeMode::Scoped);
        let scoped_index = pending.provider_index;
        pending.apply_provider_nav(-1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);
        assert_eq!(pending.provider_index, scoped_index);
    }

    #[test]
    fn scope_nav_restores_last_builtin_provider() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let indices = catalog.builtin_provider_indices();
        let second = indices.get(1).copied().unwrap_or(indices[0]);
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.last_builtin_provider_index = second;
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Free);
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Provider);
        assert_eq!(pending.provider_index, second);
    }

    #[test]
    fn scope_nav_saves_builtin_provider_when_leaving_provider_mode() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.set_scope_mode(ModelScopeMode::Provider);
        let active = pending.provider_index;
        pending.apply_scope_nav(-1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);
        assert_eq!(pending.last_builtin_provider_index, active);
    }

    #[test]
    fn scope_tab_index_maps_modes() {
        assert_eq!(scope_tab_index(ModelScopeMode::All), ALL_PROVIDERS_TAB_INDEX);
        assert_eq!(scope_tab_index(ModelScopeMode::Free), FREE_PROVIDERS_TAB_INDEX);
        assert_eq!(scope_tab_index(ModelScopeMode::Scoped), SCOPED_PROVIDERS_TAB_INDEX);
        assert_eq!(scope_tab_index(ModelScopeMode::Provider), PROVIDER_SCOPE_TAB_INDEX);
    }
}
