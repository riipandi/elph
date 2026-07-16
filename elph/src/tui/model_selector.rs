//! Model picker state, catalog snapshot, and fuzzy filtering.

use std::collections::HashMap;

use elph_ai::Model;
use elph_ai::{get_builtin_model, get_builtin_models, get_builtin_providers};
use elph_tui::components::{DialogChrome, UiTheme, dialog_max_content_height};

use crate::agent::parse_model_value;

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

/// Synthetic provider id for the curated Scoped tab (index 1).
pub const SCOPED_PROVIDERS_TAB_ID: &str = "__scoped__";

/// Tab index for settings-backed scoped models.
pub const SCOPED_PROVIDERS_TAB_INDEX: usize = 1;

/// Header label for [`SCOPED_PROVIDERS_TAB_INDEX`].
pub const SCOPED_PROVIDERS_TAB_LABEL: &str = "Scoped";

/// Header label for the Provider scope mode (third scope tab).
pub const PROVIDER_SCOPE_TAB_LABEL: &str = "Provider";

/// Index of the Provider scope tab in the 3-tab header (`All` · `Scoped` · `Provider`).
pub const PROVIDER_SCOPE_TAB_INDEX: usize = 2;

/// Number of scope tabs shown in the model picker header.
pub const SCOPE_TAB_COUNT: usize = 3;

/// Built-in provider tabs shown per header page (remaining tabs scroll via `‹ N` / `N ›`).
pub const PROVIDER_HEADER_TABS_PER_PAGE: usize = 4;

/// Catalog index where built-in provider tabs begin (after synthetic All/Scoped tabs).
pub const BUILTIN_PROVIDERS_START_INDEX: usize = 2;

/// Scope filter for the model picker header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelScopeMode {
    All,
    Scoped,
    Provider,
}

/// One provider tab in the model picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProviderTab {
    pub id: String,
    pub label: String,
    pub model_count: usize,
}

/// Catalog row with stable selection value (`provider/model_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRow {
    pub value: String,
    pub name: String,
    pub provider_id: String,
    pub model_id: String,
    pub context_k: u32,
    pub reasoning: bool,
}

fn model_row_from_builtin(provider_id: &str, model: &Model) -> ModelRow {
    ModelRow {
        value: format!("{provider_id}/{}", model.id),
        name: model.name.clone(),
        provider_id: provider_id.to_string(),
        model_id: model.id.clone(),
        context_k: model.context_window / 1000,
        reasoning: model.reasoning,
    }
}

/// Built-in model catalog for the picker UI.
#[derive(Debug, Clone)]
pub struct ModelCatalogSnapshot {
    pub providers: Vec<ModelProviderTab>,
    pub models_by_provider: HashMap<String, Vec<ModelRow>>,
    /// Every model across providers (tab [`ALL_PROVIDERS_TAB_INDEX`]).
    pub all_models: Vec<ModelRow>,
    /// Models from [`crate::platform::Settings::scoped_model_items`] (tab [`SCOPED_PROVIDERS_TAB_INDEX`]).
    pub scoped_models: Vec<ModelRow>,
    pub total_providers: usize,
    pub total_models: usize,
}

impl ModelCatalogSnapshot {
    pub fn build(scoped_model_items: &[String]) -> Self {
        let provider_ids = get_builtin_providers();
        let mut providers = Vec::new();
        let mut models_by_provider = HashMap::new();
        let mut total_models = 0usize;

        for provider_id in &provider_ids {
            let models = get_builtin_models(provider_id);
            let count = models.len();
            total_models = total_models.saturating_add(count);
            let rows: Vec<ModelRow> = models
                .into_iter()
                .map(|model| model_row_from_builtin(provider_id, &model))
                .collect();
            providers.push(ModelProviderTab {
                id: (*provider_id).to_string(),
                label: format_provider_label(provider_id),
                model_count: count,
            });
            models_by_provider.insert((*provider_id).to_string(), rows);
        }

        let total_providers = providers.len();
        let mut all_models: Vec<ModelRow> = Vec::new();
        for provider_id in &provider_ids {
            for model in get_builtin_models(provider_id) {
                all_models.push(model_row_from_builtin(provider_id, &model));
            }
        }
        all_models.sort_by(|left, right| left.name.cmp(&right.name).then_with(|| left.value.cmp(&right.value)));

        let scoped_models = build_scoped_model_rows(scoped_model_items);

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
                id: SCOPED_PROVIDERS_TAB_ID.to_string(),
                label: SCOPED_PROVIDERS_TAB_LABEL.to_string(),
                model_count: scoped_models.len(),
            },
        );
        models_by_provider.insert(ALL_PROVIDERS_TAB_ID.to_string(), all_models.clone());
        models_by_provider.insert(SCOPED_PROVIDERS_TAB_ID.to_string(), scoped_models.clone());

        Self {
            providers,
            models_by_provider,
            all_models,
            scoped_models,
            total_providers,
            total_models,
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

    pub fn is_scoped_providers_tab(&self, index: usize) -> bool {
        self.provider_id(index) == Some(SCOPED_PROVIDERS_TAB_ID)
    }

    pub fn shows_provider_in_hint(&self, provider_index: usize) -> bool {
        self.is_all_providers_tab(provider_index) || self.is_scoped_providers_tab(provider_index)
    }

    pub fn scope_mode(&self, provider_index: usize) -> ModelScopeMode {
        if self.is_all_providers_tab(provider_index) {
            ModelScopeMode::All
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
        ModelScopeMode::Scoped => SCOPED_PROVIDERS_TAB_INDEX,
        ModelScopeMode::Provider => PROVIDER_SCOPE_TAB_INDEX,
    }
}

pub fn scope_mode_from_tab_index(index: usize) -> ModelScopeMode {
    match index {
        ALL_PROVIDERS_TAB_INDEX => ModelScopeMode::All,
        SCOPED_PROVIDERS_TAB_INDEX => ModelScopeMode::Scoped,
        _ => ModelScopeMode::Provider,
    }
}

pub fn scope_tab_labels() -> [&'static str; SCOPE_TAB_COUNT] {
    [
        ALL_PROVIDERS_TAB_LABEL,
        SCOPED_PROVIDERS_TAB_LABEL,
        PROVIDER_SCOPE_TAB_LABEL,
    ]
}

pub fn is_synthetic_provider_tab(id: &str) -> bool {
    id == ALL_PROVIDERS_TAB_ID || id == SCOPED_PROVIDERS_TAB_ID
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
}

impl PendingModelSelector {
    pub fn open(initial_filter: String, stashed_prompt_draft: Option<String>, scoped_model_items: &[String]) -> Self {
        let catalog = ModelCatalogSnapshot::build(scoped_model_items);
        let last_builtin_provider_index = catalog.first_builtin_provider_index();
        Self {
            catalog,
            provider_index: ALL_PROVIDERS_TAB_INDEX,
            last_builtin_provider_index,
            model_index: 0,
            filter: initial_filter,
            stashed_prompt_draft,
            input_focus: ModelSelectorFocus::Search,
        }
    }

    pub fn open_with_selection(
        initial_filter: String,
        stashed_prompt_draft: Option<String>,
        scoped_model_items: &[String],
        provider_id: Option<&str>,
        model_id: Option<&str>,
    ) -> Self {
        let mut pending = Self::open(initial_filter, stashed_prompt_draft, scoped_model_items);
        if let (Some(provider), Some(model)) = (provider_id, model_id) {
            let value = format!("{provider}/{model}");
            let scoped_index = pending
                .catalog
                .providers
                .iter()
                .position(|tab| tab.id == SCOPED_PROVIDERS_TAB_ID)
                .filter(|_| pending.catalog.scoped_models.iter().any(|row| row.value == value));
            if let Some(pi) =
                scoped_index.or_else(|| pending.catalog.providers.iter().position(|tab| tab.id == provider))
            {
                pending.provider_index = pi;
                if !is_synthetic_provider_tab(provider) {
                    pending.last_builtin_provider_index = pi;
                }
                let models = pending.filtered_models();
                if let Some(mi) = models.iter().position(|row| row.value == value) {
                    pending.model_index = mi;
                }
            }
        }
        pending
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

    pub fn filtered_models(&self) -> Vec<ModelRow> {
        if !self.filter.trim().is_empty() {
            return filter_models_fuzzy(&self.catalog.all_models, &self.filter);
        }
        let provider = match self.active_provider_id() {
            Some(id) => id,
            None => return Vec::new(),
        };
        let models = self
            .catalog
            .models_by_provider
            .get(provider)
            .cloned()
            .unwrap_or_default();
        filter_models_fuzzy(&models, &self.filter)
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
        self.provider_index = match mode {
            ModelScopeMode::All => ALL_PROVIDERS_TAB_INDEX,
            ModelScopeMode::Scoped => SCOPED_PROVIDERS_TAB_INDEX,
            ModelScopeMode::Provider => self.last_builtin_provider_index,
        };
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

    /// `←/→` cycles scope tabs in All/Scoped; in Provider mode, moves across `All`, `Scoped`, and providers.
    pub fn apply_horizontal_nav(&mut self, delta: isize) {
        if self.is_provider_scope_mode() {
            self.apply_provider_mode_header_nav(delta);
        } else {
            self.apply_scope_nav(delta);
        }
    }

    fn provider_mode_header_segments(&self) -> Vec<usize> {
        let mut segments = vec![ALL_PROVIDERS_TAB_INDEX, SCOPED_PROVIDERS_TAB_INDEX];
        segments.extend(self.catalog.builtin_provider_indices());
        segments
    }

    fn provider_mode_header_index(&self) -> usize {
        let segments = self.provider_mode_header_segments();
        segments
            .iter()
            .position(|&index| index == self.provider_index)
            .unwrap_or_else(|| segments.len().saturating_sub(1))
    }

    fn apply_provider_mode_header_nav(&mut self, delta: isize) {
        let segments = self.provider_mode_header_segments();
        if segments.is_empty() {
            return;
        }
        let current = self.provider_mode_header_index().min(segments.len() - 1);
        let next = (current as isize + delta).rem_euclid(segments.len() as isize) as usize;
        let target = segments[next];
        if self.catalog.is_all_providers_tab(target) {
            self.set_scope_mode(ModelScopeMode::All);
        } else if self.catalog.is_scoped_providers_tab(target) {
            self.set_scope_mode(ModelScopeMode::Scoped);
        } else {
            self.last_builtin_provider_index = target;
            self.set_provider_index(target);
        }
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
    format!(
        "{} providers · {} models available",
        catalog.total_providers, catalog.total_models
    )
}

pub fn model_selector_footer_hint(in_provider_scope: bool) -> String {
    let header_hint = if in_provider_scope {
        "←/→ scope & provider"
    } else {
        "←/→ scope"
    };
    format!("↑/↓ model · {header_hint} · [ ] scope · a-z filter · / focus filter · Enter confirm · Esc cancel")
}

pub fn model_match_score(row: &ModelRow, query: &str) -> Option<i32> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Some(0);
    }

    let provider_label = format_provider_label(&row.provider_id).to_ascii_lowercase();
    let context_label = format!("{}k", row.context_k);

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

    let mut scored: Vec<(ModelRow, i32)> = models
        .iter()
        .filter_map(|row| model_row_match_score(row, query).map(|score| (row.clone(), score)))
        .collect();
    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.name.cmp(&right.0.name)));
    scored.into_iter().map(|(row, _)| row).collect()
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
    fn filter_with_query_searches_all_models_regardless_of_provider_tab() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        let anthropic = pending
            .catalog
            .providers
            .iter()
            .position(|tab| tab.id == "anthropic")
            .expect("anthropic provider");
        pending.set_provider_index(anthropic);
        pending.filter = "big-pickle".to_string();
        let filtered = pending.filtered_models();
        assert!(
            filtered.iter().any(|row| row.model_id == "big-pickle"),
            "expected global fuzzy search to find big-pickle from any provider tab"
        );
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
            },
            ModelRow {
                value: "anthropic/claude-opus-4".into(),
                name: "Claude Opus 4".into(),
                provider_id: "anthropic".into(),
                model_id: "claude-opus-4".into(),
                context_k: 200,
                reasoning: true,
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
            },
            ModelRow {
                value: "anthropic/claude-opus-4".into(),
                name: "Claude Opus 4".into(),
                provider_id: "anthropic".into(),
                model_id: "claude-opus-4".into(),
                context_k: 200,
                reasoning: true,
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
            },
            ModelRow {
                value: "a/m2".into(),
                name: "M2".into(),
                provider_id: "a".into(),
                model_id: "m2".into(),
                context_k: 128,
                reasoning: false,
            },
        ];
        assert_eq!(filter_models_fuzzy(&rows, ""), rows);
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
            scoped_models: vec![],
            total_providers: 3,
            total_models: 12,
        };
        assert_eq!(global_count_label(&catalog), "3 providers · 12 models available");
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
    fn catalog_places_scoped_tab_after_all() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        assert_eq!(catalog.providers[1].id, SCOPED_PROVIDERS_TAB_ID);
        assert_eq!(catalog.providers[1].label, SCOPED_PROVIDERS_TAB_LABEL);
        assert_eq!(catalog.scoped_models.len(), 0);
    }

    #[test]
    fn scoped_tab_lists_configured_models_in_order() {
        let base = ModelCatalogSnapshot::build(&[]);
        let sample = base.all_models.first().expect("model").value.clone();
        let catalog = ModelCatalogSnapshot::build(std::slice::from_ref(&sample));
        assert_eq!(catalog.providers[1].model_count, 1);
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
    fn open_with_selection_targets_provider_tab_not_all() {
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
        assert_eq!(pending.active_provider_id(), Some(provider_id));
        assert!(!catalog.is_all_providers_tab(pending.provider_index));
    }

    #[test]
    fn open_with_selection_prefers_scoped_tab_when_model_is_curated() {
        let base = ModelCatalogSnapshot::build(&[]);
        let sample = base.all_models.first().expect("model");
        let (provider_id, model_id) = sample.value.split_once('/').expect("provider/model");
        let catalog = ModelCatalogSnapshot::build(std::slice::from_ref(&sample.value));
        let pending = PendingModelSelector::open_with_selection(
            String::new(),
            None,
            std::slice::from_ref(&sample.value),
            Some(provider_id),
            Some(model_id),
        );
        assert!(catalog.is_scoped_providers_tab(pending.provider_index));
        assert_eq!(pending.selected_model().map(|row| row.value), Some(sample.value.clone()));
    }

    #[test]
    fn horizontal_nav_cycles_scope_when_not_in_provider_mode() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
        pending.apply_horizontal_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);
        pending.apply_horizontal_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Provider);
    }

    #[test]
    fn horizontal_nav_cycles_builtin_providers_in_provider_mode() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let builtins = catalog.builtin_provider_indices();
        assert!(builtins.len() >= 2);
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.set_scope_mode(ModelScopeMode::Provider);
        let start = pending.provider_index;
        pending.apply_horizontal_nav(1);
        assert_ne!(pending.provider_index, start);
        assert!(pending.is_provider_scope_mode());
        assert_eq!(pending.last_builtin_provider_index, pending.provider_index);
    }

    #[test]
    fn horizontal_nav_from_first_provider_reaches_scoped() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let first = catalog
            .builtin_provider_indices()
            .first()
            .copied()
            .expect("builtin provider");
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.set_provider_index(first);
        assert!(pending.is_provider_scope_mode());
        pending.apply_horizontal_nav(-1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);
    }

    #[test]
    fn horizontal_nav_from_provider_mode_reaches_all() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let first = catalog
            .builtin_provider_indices()
            .first()
            .copied()
            .expect("builtin provider");
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.set_provider_index(first);
        pending.apply_horizontal_nav(-1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);
        pending.apply_horizontal_nav(-1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::All);
    }

    #[test]
    fn horizontal_nav_from_scoped_enters_first_provider() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let first = catalog
            .builtin_provider_indices()
            .first()
            .copied()
            .expect("builtin provider");
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.set_scope_mode(ModelScopeMode::Scoped);
        pending.apply_horizontal_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Provider);
        assert_eq!(pending.provider_index, first);
    }

    #[test]
    fn scope_nav_restores_last_builtin_provider() {
        let catalog = ModelCatalogSnapshot::build(&[]);
        let indices = catalog.builtin_provider_indices();
        let second = indices.get(1).copied().unwrap_or(indices[0]);
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.last_builtin_provider_index = second;
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
        assert_eq!(scope_tab_index(ModelScopeMode::Scoped), SCOPED_PROVIDERS_TAB_INDEX);
        assert_eq!(scope_tab_index(ModelScopeMode::Provider), PROVIDER_SCOPE_TAB_INDEX);
    }
}
