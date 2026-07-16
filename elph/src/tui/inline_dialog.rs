//! Full-width inline dialog shell (matches prompt editor chrome).

use elph_tui::components::{UiTheme, dialog_header_title_fit};
use iocraft::prelude::*;

use crate::tui::model_selector::PROVIDER_HEADER_TABS_PER_PAGE;
use crate::tui::user_question::{QuestionStepTab, QuestionStepTabState};

/// Gap between sections inside inline dialog bodies (tighter than modal dialogs).
pub const INLINE_SECTION_GAP: u16 = 0;

/// Space above the first selectable answer row in inline dialogs.
pub const OPTIONS_LIST_TOP_GAP: u16 = 1;

/// Inner content width inside the round border and zone padding.
pub fn inline_body_width(screen_width: u16) -> u16 {
    UiTheme::default().shell_editor_inner_width(screen_width)
}

/// Visual state for a header tab chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineDialogTabState {
    Current,
    Answered,
    Upcoming,
}

impl From<QuestionStepTabState> for InlineDialogTabState {
    fn from(state: QuestionStepTabState) -> Self {
        match state {
            QuestionStepTabState::Current => Self::Current,
            QuestionStepTabState::Answered => Self::Answered,
            QuestionStepTabState::Upcoming => Self::Upcoming,
        }
    }
}

/// One navigable tab in the dialog header (not keyboard Tab).
#[derive(Clone, Debug)]
pub struct InlineDialogTab {
    pub index: usize,
    pub state: InlineDialogTabState,
}

impl From<QuestionStepTab> for InlineDialogTab {
    fn from(tab: QuestionStepTab) -> Self {
        Self {
            index: tab.index,
            state: tab.state.into(),
        }
    }
}

/// Visible slice of a horizontally scrollable provider tab row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderTabWindow {
    pub window_start: usize,
    pub visible_indices: Vec<usize>,
    pub hidden_left: usize,
    pub hidden_right: usize,
}

/// Compute a fixed-size page of provider tab indices, keeping `selected_index` visible.
pub fn provider_tab_row_window(label_widths: &[usize], max_visible: usize, selected_index: usize) -> ProviderTabWindow {
    let len = label_widths.len();
    if len == 0 {
        return ProviderTabWindow {
            window_start: 0,
            visible_indices: Vec::new(),
            hidden_left: 0,
            hidden_right: 0,
        };
    }

    let selected = selected_index.min(len - 1);
    let page_size = max_visible.max(1).min(len);
    let window_start = if len <= page_size {
        0
    } else {
        let lead = page_size / 2;
        selected.saturating_sub(lead).min(len.saturating_sub(page_size))
    };
    let window_end = window_start.saturating_add(page_size);

    ProviderTabWindow {
        window_start,
        visible_indices: (window_start..window_end).collect(),
        hidden_left: window_start,
        hidden_right: len.saturating_sub(window_end),
    }
}

pub fn render_provider_tab_row(
    labels: &[String],
    selected_index: usize,
    inner: u16,
    theme: UiTheme,
    max_visible: usize,
) -> AnyElement<'static> {
    let widths: Vec<usize> = labels.iter().map(|label| label.chars().count()).collect();
    let window = provider_tab_row_window(&widths, max_visible, selected_index);

    let mut segments: Vec<AnyElement<'static>> = Vec::new();
    if window.hidden_left > 0 {
        segments.push(
            element! {
                Text(
                    content: format!("‹ {}", window.hidden_left),
                    color: theme.text_hint,
                    wrap: TextWrap::NoWrap,
                )
            }
            .into(),
        );
        segments.push(
            element! {
                Text(content: " ".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
            }
            .into(),
        );
    }

    for (pos, index) in window.visible_indices.iter().enumerate() {
        if pos > 0 || window.hidden_left > 0 {
            segments.push(
                element! {
                    Text(content: " | ".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
                }
                .into(),
            );
        }
        let active = *index == selected_index;
        let color = if active { theme.warning } else { theme.text_secondary };
        segments.push(
            element! {
                Text(
                    content: labels[*index].clone(),
                    color: color,
                    weight: if active { Weight::Bold } else { Weight::Normal },
                    wrap: TextWrap::NoWrap,
                )
            }
            .into(),
        );
    }

    if window.hidden_right > 0 {
        segments.push(
            element! {
                Text(content: " | ".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
            }
            .into(),
        );
        segments.push(
            element! {
                Text(
                    content: format!("{} ›", window.hidden_right),
                    color: theme.text_hint,
                    wrap: TextWrap::NoWrap,
                )
            }
            .into(),
        );
    }

    element! {
        View(width: inner, flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::NoWrap, flex_shrink: 0f32) {
            #(segments)
        }
    }
    .into()
}

const MODEL_SCOPE_PROVIDER_SEPARATOR: &str = " · ";

fn tab_segment_width(label_widths: &[usize], start: usize, end: usize) -> usize {
    if start >= end {
        return 0;
    }
    let separator_width = 3usize; // ` | `
    let labels: usize = (start..end).map(|i| label_widths[i]).sum();
    let separators = end.saturating_sub(start + 1);
    labels.saturating_add(separators.saturating_mul(separator_width))
}

/// Header for the model picker: `All · Scoped · Provider` and, in provider mode,
/// built-in providers on the same row (`All | Scoped | Provider · Anthropic | OpenAI`).
pub fn render_model_scope_header(
    scope_labels: &[String],
    scope_tab_index: usize,
    provider_labels: Option<&[String]>,
    provider_selected_index: Option<usize>,
    inner: u16,
    theme: UiTheme,
) -> AnyElement<'static> {
    let scope_widths: Vec<usize> = scope_labels.iter().map(|label| label.chars().count()).collect();
    let scope_width = tab_segment_width(&scope_widths, 0, scope_labels.len());
    let scope_row = render_provider_tab_row(
        scope_labels,
        scope_tab_index,
        scope_width.max(1) as u16,
        theme,
        scope_labels.len(),
    );
    let Some(provider_labels) = provider_labels.filter(|labels| !labels.is_empty()) else {
        return scope_row;
    };

    let separator_width = MODEL_SCOPE_PROVIDER_SEPARATOR.chars().count();
    let provider_inner = inner
        .max(1)
        .saturating_sub((scope_width + separator_width) as u16)
        .max(8);
    let provider_selected = provider_selected_index
        .unwrap_or(0)
        .min(provider_labels.len().saturating_sub(1));
    let provider_labels_owned: Vec<String> = provider_labels.to_vec();
    let provider_row = render_provider_tab_row(
        &provider_labels_owned,
        provider_selected,
        provider_inner,
        theme,
        PROVIDER_HEADER_TABS_PER_PAGE,
    );

    element! {
        View(width: inner, flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::NoWrap, flex_shrink: 0f32) {
            View(flex_shrink: 0f32) {
                #(scope_row)
            }
            Text(
                content: MODEL_SCOPE_PROVIDER_SEPARATOR.to_string(),
                color: theme.text_muted,
                wrap: TextWrap::NoWrap,
            )
            View(flex_grow: 1f32, flex_shrink: 1f32, min_width: provider_inner) {
                #(provider_row)
            }
        }
    }
    .into()
}

/// Props for [`InlineDialogShell`].
#[derive(Props)]
pub struct InlineDialogShellProps<'a> {
    pub screen_width: u16,
    pub title: String,
    pub has_focus: bool,
    pub tabs: Option<Vec<InlineDialogTab>>,
    /// Replaces title/step tabs when set (e.g. scrollable provider tabs).
    pub header_override: Option<AnyElement<'a>>,
    pub footer_hint: Option<String>,
    pub children: Vec<AnyElement<'a>>,
}

impl<'a> Default for InlineDialogShellProps<'a> {
    fn default() -> Self {
        Self {
            screen_width: 80,
            title: String::new(),
            has_focus: false,
            tabs: None,
            header_override: None,
            footer_hint: None,
            children: Vec::new(),
        }
    }
}

fn step_tab_label(index: usize) -> String {
    format!("Step {}", index + 1)
}

fn tab_text_color(theme: UiTheme, state: InlineDialogTabState) -> Color {
    match state {
        InlineDialogTabState::Current => theme.warning,
        InlineDialogTabState::Answered => theme.text_secondary,
        InlineDialogTabState::Upcoming => theme.text_muted,
    }
}

fn render_tab_row(tabs: &[InlineDialogTab], inner: u16, theme: UiTheme) -> AnyElement<'static> {
    let mut segments: Vec<AnyElement<'static>> = Vec::new();
    for (i, tab) in tabs.iter().enumerate() {
        if i > 0 {
            segments.push(
                element! {
                    Text(content: " | ".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
                }
                .into(),
            );
        }
        let current = tab.state == InlineDialogTabState::Current;
        segments.push(
            element! {
                Text(
                    content: step_tab_label(tab.index),
                    color: tab_text_color(theme, tab.state),
                    weight: if current { Weight::Bold } else { Weight::Normal },
                    wrap: TextWrap::NoWrap,
                )
            }
            .into(),
        );
    }
    element! {
        View(width: inner, flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::NoWrap, flex_shrink: 0f32) {
            #(segments)
        }
    }
    .into()
}

/// Single bordered frame for inline agent dialogs: full terminal width, reset background.
#[component]
pub fn InlineDialogShell<'a>(props: &mut InlineDialogShellProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let _ = hooks;
    let theme = UiTheme::default();
    let border_color = theme.shell_zone_border_color(props.has_focus);
    let inset = theme.shell_zone_padding();
    let inner = inline_body_width(props.screen_width);
    let title = dialog_header_title_fit(&props.title, inner, "");
    let divider = "─".repeat(inner.max(1) as usize);
    let children = std::mem::take(&mut props.children);
    let tabs = props.tabs.clone();
    let header_override = props.header_override.take();
    let footer_hint = props.footer_hint.clone();

    let header = if let Some(custom) = header_override {
        element! {
            View(width: inner, flex_shrink: 0f32) {
                #(custom)
            }
        }
    } else if let Some(ref tab_row) = tabs {
        if tab_row.is_empty() {
            element! {
                View(width: inner, flex_shrink: 0f32) {
                    Text(
                        content: title,
                        color: theme.text_primary,
                        weight: Weight::Bold,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
        } else {
            element! {
                View(width: inner, flex_shrink: 0f32) {
                    #(render_tab_row(tab_row, inner, theme))
                }
            }
        }
    } else {
        element! {
            View(width: inner, flex_shrink: 0f32) {
                Text(
                    content: title,
                    color: theme.text_primary,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                )
            }
        }
    };

    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            border_style: BorderStyle::Round,
            border_color: border_color,
            background_color: Color::Reset,
            position: Position::Relative,
            padding_left: inset,
            padding_right: inset,
            flex_direction: FlexDirection::Column,
            gap: 0,
        ) {
            #(header)
            View(width: inner, flex_shrink: 0f32) {
                Text(
                    content: divider,
                    color: theme.text_muted,
                    wrap: TextWrap::NoWrap,
                )
            }
            View(
                width: inner,
                flex_direction: FlexDirection::Column,
                flex_shrink: 0f32,
            ) {
                #(children)
            }
            #(footer_hint.map(|hint| -> AnyElement<'static> {
                element! {
                    View(width: inner, padding_top: 1, flex_shrink: 0f32) {
                        Text(
                            content: hint,
                            color: theme.text_muted,
                            wrap: TextWrap::Wrap,
                        )
                    }
                }
                .into()
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_tab_window_keeps_selection_visible() {
        let widths: Vec<usize> = (0..10).map(|i| 8 + i).collect();
        let window = provider_tab_row_window(&widths, PROVIDER_HEADER_TABS_PER_PAGE, 7);
        assert!(window.visible_indices.contains(&7));
    }

    #[test]
    fn provider_tab_window_limits_visible_count() {
        let widths = vec![12, 14, 16, 18, 20, 22];
        let window = provider_tab_row_window(&widths, PROVIDER_HEADER_TABS_PER_PAGE, 0);
        assert_eq!(window.visible_indices.len(), PROVIDER_HEADER_TABS_PER_PAGE);
        assert_eq!(window.hidden_right, widths.len() - PROVIDER_HEADER_TABS_PER_PAGE);
    }

    #[test]
    fn provider_tab_window_scrolls_to_selected_page() {
        let widths: Vec<usize> = (0..10).map(|_| 10).collect();
        let window = provider_tab_row_window(&widths, PROVIDER_HEADER_TABS_PER_PAGE, 8);
        assert!(window.visible_indices.contains(&8));
        assert_eq!(window.visible_indices.len(), PROVIDER_HEADER_TABS_PER_PAGE);
        assert_eq!(window.hidden_left, 6);
        assert_eq!(window.hidden_right, 0);
    }

    #[test]
    fn provider_tab_window_empty_labels() {
        let window = provider_tab_row_window(&[], PROVIDER_HEADER_TABS_PER_PAGE, 0);
        assert!(window.visible_indices.is_empty());
        assert_eq!(window.hidden_left, 0);
        assert_eq!(window.hidden_right, 0);
    }

    #[test]
    fn tab_segment_width_counts_labels_and_separators() {
        let widths = vec![3, 6, 8];
        assert_eq!(tab_segment_width(&widths, 0, 3), 23);
        assert_eq!(tab_segment_width(&widths, 0, 1), 3);
    }
}
