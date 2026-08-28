//! Inline model picker above the status row.

use elph_tui::components::UiTheme;
use iocraft::prelude::*;

use crate::tui::inline_dialog::{
    InlineDialogShell, OPTIONS_LIST_TOP_GAP, inline_body_width, render_model_scope_header,
};
use crate::tui::model_option_list::ModelOptionList;
use crate::tui::model_selector::{
    ModelCatalogSnapshot, ModelRow, ModelScopeMode, PendingModelSelector, model_selector_footer_hint,
    model_selector_list_viewport_height, model_selector_status_label, scope_tab_index, scope_tab_labels,
};

/// Render snapshot for [`ModelSelectorBar`].
#[derive(Debug, Clone)]
pub struct ModelSelectorView {
    pub catalog: ModelCatalogSnapshot,
    pub provider_index: usize,
    pub filtered_models: Vec<ModelRow>,
    pub status: String,
    pub footer_hint: String,
}

impl ModelSelectorView {
    pub fn from_pending(pending: &PendingModelSelector) -> Self {
        let filtered_models = pending.filtered_models();
        let model_count = filtered_models.len();
        Self {
            catalog: pending.catalog.clone(),
            provider_index: pending.provider_index,
            filtered_models,
            status: model_selector_status_label(&pending.filter, model_count),
            footer_hint: model_selector_footer_hint(pending.is_provider_scope_mode(), pending.sort_order),
        }
    }

    pub fn scope_tab_index(&self) -> usize {
        scope_tab_index(self.catalog.scope_mode(self.provider_index))
    }

    pub fn builtin_provider_labels(&self) -> Option<Vec<String>> {
        if !matches!(self.catalog.scope_mode(self.provider_index), ModelScopeMode::Provider) {
            return None;
        }
        let labels = self
            .catalog
            .builtin_provider_indices()
            .into_iter()
            .filter_map(|index| self.catalog.providers.get(index).map(|tab| tab.label.clone()))
            .collect::<Vec<_>>();
        if labels.is_empty() { None } else { Some(labels) }
    }

    pub fn builtin_provider_tab_index(&self) -> Option<usize> {
        if !matches!(self.catalog.scope_mode(self.provider_index), ModelScopeMode::Provider) {
            return None;
        }
        let indices = self.catalog.builtin_provider_indices();
        indices.iter().position(|&index| index == self.provider_index)
    }
}

#[derive(Props)]
pub struct ModelSelectorBarProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub view: ModelSelectorView,
    pub model_index: Option<State<usize>>,
    pub has_focus: bool,
}

impl Default for ModelSelectorBarProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            view: ModelSelectorView {
                catalog: ModelCatalogSnapshot::build_with_options(
                    &[],
                    &super::model_selector::ModelCatalogOptions::unfiltered(),
                ),
                provider_index: 0,
                filtered_models: Vec::new(),
                status: String::new(),
                footer_hint: String::new(),
            },
            model_index: None,
            has_focus: false,
        }
    }
}

#[component]
pub fn ModelSelectorBar(props: &mut ModelSelectorBarProps, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);
    let scope_labels: Vec<String> = scope_tab_labels().iter().map(|label| (*label).to_string()).collect();
    let header_tabs = render_model_scope_header(
        &scope_labels,
        props.view.scope_tab_index(),
        props.view.builtin_provider_labels().as_deref(),
        props.view.builtin_provider_tab_index(),
        body_width,
        theme,
    );

    let list_height = model_selector_list_viewport_height(props.screen_width, props.screen_height);
    let body = element! {
        View(
            width: body_width,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            Text(
                content: props.view.status.clone(),
                color: theme.text_muted,
                wrap: TextWrap::NoWrap,
            )
            View(width: body_width, padding_top: OPTIONS_LIST_TOP_GAP, flex_shrink: 0f32) {
                ModelOptionList(
                    width: body_width,
                    height: list_height,
                    models: props.view.filtered_models.clone(),
                    show_provider_hint: true,
                    selected_index: props.model_index,
                    has_focus: props.has_focus,
                    theme: Some(theme),
                )
            }
        }
    };

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: "Select model".to_string(),
            has_focus: props.has_focus,
            header_override: Some(header_tabs),
            footer_hint: Some(props.view.footer_hint.clone()),
        ) {
            #(body)
        }
    }
}
