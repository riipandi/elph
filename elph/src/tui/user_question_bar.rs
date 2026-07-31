//! Inline ask-user dialog above the status row.

use elph_tui::components::{
    ConfirmButtonFocus, DIALOG_SELECT_AUTO_HEIGHT, DialogMultiChoiceContent, DialogUserInputContent, UiTheme,
    dialog_body_min_height, dialog_header_title_fit, dialog_max_content_height,
};
use elph_tui::types::SelectOption;
use iocraft::prelude::*;

use crate::agent::UserQuestionOption;
use crate::tui::inline_dialog::{
    INLINE_SECTION_GAP, InlineDialogShell, InlineDialogTab, OPTIONS_LIST_TOP_GAP, inline_body_width,
};
use crate::tui::user_question::{
    PendingUserQuestion, QuestionInputFocus, question_footer_hint, user_question_select_options,
};
use crate::tui::user_question_option_list::{UserQuestionOptionList, option_list_total_rows_with_custom};

/// Snapshot for rendering the active ask-user step.
#[derive(Debug, Clone, Default)]
pub struct UserQuestionView {
    pub step_count: usize,
    pub question: String,
    pub is_confirm: bool,
    pub is_multi_select: bool,
    pub allow_custom: bool,
    pub custom_label: String,
    pub options: Option<Vec<UserQuestionOption>>,
    pub tabs: Vec<InlineDialogTab>,
    pub review_summary: Vec<String>,
    pub footer_hint: String,
}

impl UserQuestionView {
    pub fn from_pending(
        pending: &PendingUserQuestion,
        input_focus: QuestionInputFocus,
        selected_index: usize,
        multi_checked: &[bool],
        validation_error: Option<String>,
    ) -> Self {
        Self {
            step_count: pending.step_count(),
            question: pending.question().to_string(),
            is_confirm: pending.is_confirm(),
            is_multi_select: pending.is_multi_select(),
            allow_custom: pending.allow_custom(),
            custom_label: pending.custom_label().to_string(),
            options: pending.options().map(|options| options.to_vec()),
            tabs: pending.step_tabs().into_iter().map(InlineDialogTab::from).collect(),
            review_summary: pending.review_summary_lines(),
            footer_hint: question_footer_hint(
                pending,
                input_focus,
                selected_index,
                multi_checked,
                validation_error.as_deref(),
            ),
        }
    }
}

/// Props for [`UserQuestionBar`].
#[derive(Props)]
pub struct UserQuestionBarProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub view: UserQuestionView,
    pub selected_index: Option<State<usize>>,
    pub multi_checked: Option<State<Vec<bool>>>,
    pub confirm_focus: Option<State<ConfirmButtonFocus>>,
    pub answer: Option<State<String>>,
    pub input_focus: QuestionInputFocus,
    pub has_focus: bool,
    pub on_confirm_yes: HandlerMut<'static, ()>,
    pub on_confirm_no: HandlerMut<'static, ()>,
    pub on_text_submit: HandlerMut<'static, ()>,
    pub on_text_cancel: HandlerMut<'static, ()>,
}

impl Default for UserQuestionBarProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            view: UserQuestionView::default(),
            selected_index: None,
            multi_checked: None,
            confirm_focus: None,
            answer: None,
            input_focus: QuestionInputFocus::Choices,
            has_focus: false,
            on_confirm_yes: HandlerMut::default(),
            on_confirm_no: HandlerMut::default(),
            on_text_submit: HandlerMut::default(),
            on_text_cancel: HandlerMut::default(),
        }
    }
}

fn question_title(view: &UserQuestionView, body_width: u16) -> String {
    if view.step_count > 1 {
        return String::new();
    }
    if view.is_confirm {
        return "Confirm".to_string();
    }
    let prompt = view.question.lines().next().unwrap_or(&view.question);
    dialog_header_title_fit(prompt, body_width, "")
}

fn question_list_height(
    screen_width: u16,
    screen_height: u16,
    view: &UserQuestionView,
    body_width: u16,
    option_count: usize,
    custom_input_active: bool,
) -> u16 {
    let theme = UiTheme::default();
    let chrome = elph_tui::components::DialogChrome::from_theme(theme, screen_width);
    let max_body = dialog_max_content_height(screen_height, &chrome, 16);

    if view.is_confirm {
        let rows = view.question.lines().count() as u16;
        let _ = dialog_body_min_height(7u16.saturating_add(rows).min(max_body));
        return DIALOG_SELECT_AUTO_HEIGHT;
    }

    if option_count > 0 {
        let select_options = user_question_select_options(
            view.options.as_ref().unwrap_or(&vec![]),
            view.allow_custom,
            &view.custom_label,
        );
        let custom_row = view
            .allow_custom
            .then(|| select_options.len().saturating_sub(1))
            .filter(|_| !select_options.is_empty());
        // Inline custom field lives inside the list on single-select steps.
        let inline_custom = custom_input_active && !view.is_multi_select;
        let list_h = option_list_total_rows_with_custom(&select_options, body_width, custom_row, inline_custom);
        return list_h.min(max_body).max(1);
    }

    DIALOG_SELECT_AUTO_HEIGHT
}

fn render_review_summary(body_width: u16, lines: &[String], theme: UiTheme) -> Option<AnyElement<'static>> {
    if lines.is_empty() {
        return None;
    }
    let text = lines.join("\n");
    Some(
        element! {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                flex_shrink: 0f32,
            ) {
                Text(
                    content: "Your answers so far:".to_string(),
                    color: theme.text_muted,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                )
                Text(
                    content: text,
                    color: theme.text_secondary,
                    wrap: TextWrap::Wrap,
                )
            }
        }
        .into(),
    )
}

pub(crate) fn custom_input_placeholder(custom_label: &str) -> String {
    let trimmed = custom_label.trim();
    if trimmed.is_empty() || trimmed == "Other…" {
        "Type your answer…".to_string()
    } else {
        trimmed.to_string()
    }
}

fn render_multi_custom_input(
    props: &mut UserQuestionBarProps,
    body_width: u16,
    theme: UiTheme,
    custom_focused: bool,
) -> AnyElement<'static> {
    let indent = 6u16;
    let field_width = body_width.saturating_sub(indent).max(1);
    let placeholder = custom_input_placeholder(&props.view.custom_label);
    element! {
        View(
            width: body_width,
            padding_left: indent,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            DialogUserInputContent(
                width: field_width,
                question: String::new(),
                placeholder: placeholder,
                value: props.answer,
                has_focus: props.has_focus && custom_focused,
                theme: Some(theme),
                section_gap: Some(0),
                show_prompt: false,
                show_footer_hint: false,
                show_placeholder_when_focused: true,
                dialog_chrome: true,
                compact: true,
                on_submit: props.on_text_submit.take(),
                on_cancel: props.on_text_cancel.take(),
            )
        }
    }
    .into()
}

fn render_prompt_line(body_width: u16, question: &str, theme: UiTheme) -> AnyElement<'static> {
    element! {
        View(width: body_width, flex_shrink: 0f32) {
            Text(
                content: question.to_string(),
                color: theme.text_secondary,
                wrap: TextWrap::Wrap,
            )
        }
    }
    .into()
}

fn render_question_body(props: &mut UserQuestionBarProps, body_width: u16, list_height: u16) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let has_focus = props.has_focus;
    let view = &props.view;
    let choices_focused = has_focus && props.input_focus.is_choices();
    let custom_focused = has_focus && props.input_focus.is_custom();
    let show_custom_input = view.allow_custom && custom_focused;
    let summary = render_review_summary(body_width, &view.review_summary, theme);

    if view.is_confirm {
        let yes_no_label = if view.step_count > 1 {
            String::new()
        } else {
            view.question.clone()
        };
        let confirm_options = vec![
            SelectOption::new("Yes", "y".to_string()),
            SelectOption::new("No", "n".to_string()),
        ];
        return element! {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(summary)
                #(if !yes_no_label.is_empty() {
                    Some(render_prompt_line(body_width, &yes_no_label, theme))
                } else {
                    None
                })
                View(width: body_width, padding_top: OPTIONS_LIST_TOP_GAP, flex_shrink: 0f32) {
                    UserQuestionOptionList(
                        width: body_width,
                        height: list_height,
                        options: confirm_options,
                        selected_index: props.selected_index,
                        has_focus: choices_focused,
                        theme: Some(theme),
                        custom_row_index: None,
                        custom_input_active: false,
                        custom_answer: None,
                        custom_input_placeholder: String::new(),
                        custom_input_focused: false,
                        on_custom_submit: HandlerMut::default(),
                        on_custom_cancel: HandlerMut::default(),
                    )
                }
            }
        }
        .into();
    }

    if view.is_multi_select {
        let select_options = user_question_select_options(
            view.options.as_ref().unwrap_or(&vec![]),
            view.allow_custom,
            &view.custom_label,
        );
        return element! {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(summary)
                #(render_prompt_line(body_width, &view.question, theme))
                View(width: body_width, padding_top: OPTIONS_LIST_TOP_GAP, flex_shrink: 0f32) {
                    DialogMultiChoiceContent(
                        width: body_width,
                        height: list_height,
                        question: String::new(),
                        options: select_options,
                        cursor_index: props.selected_index,
                        checked: props.multi_checked,
                        has_focus: choices_focused,
                        show_description: true,
                        inline_description: true,
                        show_footer_hint: false,
                        theme: Some(theme),
                        on_submit: HandlerMut::default(),
                    )
                }
                #(if show_custom_input {
                    Some(render_multi_custom_input(props, body_width, theme, custom_focused))
                } else {
                    None
                })
            }
        }
        .into();
    }

    if let Some(options) = view.options.as_ref() {
        let select_options = user_question_select_options(options, view.allow_custom, &view.custom_label);
        let custom_row_index = view
            .allow_custom
            .then(|| select_options.len().saturating_sub(1))
            .filter(|_| !select_options.is_empty());
        let custom_placeholder = custom_input_placeholder(&view.custom_label);
        return element! {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(summary)
                #(render_prompt_line(body_width, &view.question, theme))
                View(width: body_width, padding_top: OPTIONS_LIST_TOP_GAP, flex_shrink: 0f32) {
                    UserQuestionOptionList(
                        width: body_width,
                        height: list_height,
                        options: select_options,
                        selected_index: props.selected_index,
                        has_focus: choices_focused,
                        theme: Some(theme),
                        custom_row_index: custom_row_index,
                        custom_input_active: show_custom_input,
                        custom_answer: props.answer,
                        custom_input_placeholder: custom_placeholder,
                        custom_input_focused: props.has_focus && custom_focused,
                        on_custom_submit: props.on_text_submit.take(),
                        on_custom_cancel: props.on_text_cancel.take(),
                    )
                }
            }
        }
        .into();
    }

    element! {
        View(
            width: body_width,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            #(summary)
            DialogUserInputContent(
                width: body_width,
                question: view.question.clone(),
                placeholder: "Type your answer…".to_string(),
                value: props.answer,
                has_focus: has_focus,
                theme: Some(theme),
                section_gap: Some(0),
                show_prompt: false,
                show_footer_hint: false,
                show_placeholder_when_focused: false,
                dialog_chrome: true,
                compact: true,
                on_submit: props.on_text_submit.take(),
                on_cancel: props.on_text_cancel.take(),
            )
        }
    }
    .into()
}

#[component]
pub fn UserQuestionBar(props: &mut UserQuestionBarProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let body_width = inline_body_width(props.screen_width);
    let option_count = props.view.options.as_ref().map_or(0, |options| {
        options
            .len()
            .saturating_add(usize::from(props.view.allow_custom && !options.is_empty()))
    });
    let custom_input_active = props.view.allow_custom && props.input_focus.is_custom();
    let list_height = question_list_height(
        props.screen_width,
        props.screen_height,
        &props.view,
        body_width,
        option_count,
        custom_input_active,
    );
    let title = question_title(&props.view, body_width);
    let tabs = if props.view.step_count > 1 {
        Some(props.view.tabs.clone())
    } else {
        None
    };
    let footer_hint = Some(props.view.footer_hint.clone());
    let body = render_question_body(props, body_width, list_height);

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: title,
            has_focus: props.has_focus,
            tabs: tabs,
            footer_hint: footer_hint,
        ) {
            #(body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::custom_input_placeholder;

    #[test]
    fn custom_input_placeholder_uses_agent_label_for_remarks() {
        assert_eq!(custom_input_placeholder("Remarks"), "Remarks");
        assert_eq!(custom_input_placeholder("  Notes  "), "Notes");
    }

    #[test]
    fn custom_input_placeholder_defaults_for_other() {
        assert_eq!(custom_input_placeholder("Other…"), "Type your answer…");
        assert_eq!(custom_input_placeholder(""), "Type your answer…");
    }
}
