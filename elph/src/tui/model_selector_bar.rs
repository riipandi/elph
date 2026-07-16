//! Inline model picker above the status row.

use elph_tui::components::{DialogUserInputContent, UiTheme};
use iocraft::prelude::*;

use crate::tui::inline_dialog::{
    InlineDialogShell, OPTIONS_LIST_TOP_GAP, inline_body_width, render_model_scope_header,
};
use crate::tui::model_option_list::ModelOptionList;
use crate::tui::model_selector::{
    ModelCatalogSnapshot, ModelRow, ModelScopeMode, ModelSelectorFocus, PendingModelSelector, global_count_label,
    model_selector_footer_hint, model_selector_list_viewport_height, scope_tab_index, scope_tab_labels,
};

/// Render snapshot for [`ModelSelectorBar`].
#[derive(Debug, Clone)]
pub struct ModelSelectorView {
    pub catalog: ModelCatalogSnapshot,
    pub provider_index: usize,
    pub filtered_models: Vec<ModelRow>,
    pub global_count: String,
    pub footer_hint: String,
}

impl ModelSelectorView {
    pub fn from_pending(pending: &PendingModelSelector) -> Self {
        let filtered_models = pending.filtered_models();
        Self {
            catalog: pending.catalog.clone(),
            provider_index: pending.provider_index,
            global_count: global_count_label(&pending.catalog),
            filtered_models: filtered_models.clone(),
            footer_hint: model_selector_footer_hint(pending.is_provider_scope_mode()),
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

    pub fn shows_provider_in_list_hint(&self, filter: &str) -> bool {
        !filter.trim().is_empty() || self.catalog.shows_provider_in_hint(self.provider_index)
    }
}

#[derive(Props)]
pub struct ModelSelectorBarProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub view: ModelSelectorView,
    pub provider_index: Option<State<usize>>,
    pub model_index: Option<State<usize>>,
    pub filter: Option<State<String>>,
    pub input_focus: ModelSelectorFocus,
    pub has_focus: bool,
    pub on_filter_submit: HandlerMut<'static, ()>,
    pub on_confirm: HandlerMut<'static, ()>,
    pub on_cancel: HandlerMut<'static, ()>,
}

impl Default for ModelSelectorBarProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            view: ModelSelectorView {
                catalog: ModelCatalogSnapshot::build(&[]),
                provider_index: 0,
                filtered_models: Vec::new(),
                global_count: String::new(),
                footer_hint: String::new(),
            },
            provider_index: None,
            model_index: None,
            filter: None,
            input_focus: ModelSelectorFocus::Search,
            has_focus: false,
            on_filter_submit: HandlerMut::default(),
            on_confirm: HandlerMut::default(),
            on_cancel: HandlerMut::default(),
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
    let filter_text = props
        .filter
        .as_ref()
        .map(|state| state.read().clone())
        .unwrap_or_default();
    let show_provider_hint = props.view.shows_provider_in_list_hint(&filter_text);
    let search_focused = props.has_focus && props.input_focus == ModelSelectorFocus::Search;

    let body = element! {
        View(
            width: body_width,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            Text(
                content: props.view.global_count.clone(),
                color: theme.text_muted,
                wrap: TextWrap::NoWrap,
            )
            View(width: body_width, padding_top: 1, flex_shrink: 0f32) {
                DialogUserInputContent(
                    width: body_width,
                    question: String::new(),
                    placeholder: "Filter models…".to_string(),
                    value: props.filter,
                    has_focus: search_focused,
                    theme: Some(theme),
                    show_prompt: false,
                    show_footer_hint: false,
                    show_placeholder_when_focused: true,
                    dialog_chrome: true,
                    compact: true,
                    on_submit: props.on_filter_submit.take(),
                    on_cancel: props.on_cancel.take(),
                )
            }
            View(width: body_width, padding_top: OPTIONS_LIST_TOP_GAP, flex_shrink: 0f32) {
                ModelOptionList(
                    width: body_width,
                    height: list_height,
                    models: props.view.filtered_models.clone(),
                    show_provider_hint: show_provider_hint,
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
