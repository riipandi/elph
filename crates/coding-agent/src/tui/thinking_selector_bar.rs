//! Inline bar UI for [`super::thinking_selector::PendingThinkingSelector`].
//!
//! Full-width list rows (same stretch as the slash command palette rows): each row
//! spans the content width, selected row shows the `❯` marker. Descriptions stay
//! dimmed so the level names remain the visual focus.

use elph_tui::components::theme::{UiTheme, dialog_option_name_style, dialog_row_surface};
use elph_tui::list_selection_row_prefix;
use iocraft::prelude::*;

use crate::tui::inline_dialog::{InlineDialogShell, OPTIONS_LIST_TOP_GAP, inline_body_width};
use crate::tui::thinking_selector::PendingThinkingSelector;
use crate::types::ThinkingLevel;

const FOOTER_HINT: &str = "↑↓ move · Enter set · Esc cancel";

#[derive(Props)]
pub struct ThinkingSelectorBarProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub has_focus: bool,
    pub pending: Option<PendingThinkingSelector>,
    /// SelectList highlight — update only on open / keys, never during render.
    pub selected_index: Option<State<usize>>,
    pub on_cancel: HandlerMut<'static, ()>,
}

impl Default for ThinkingSelectorBarProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            has_focus: false,
            pending: None,
            selected_index: None,
            on_cancel: HandlerMut::default(),
        }
    }
}

fn thinking_level_hint(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::Off => "No reasoning",
        ThinkingLevel::Minimal => "Very brief reasoning (~1k tokens)",
        ThinkingLevel::Low => "Light reasoning (~2k tokens)",
        ThinkingLevel::Medium => "Moderate reasoning (~8k tokens)",
        ThinkingLevel::High => "Deep reasoning (~16k tokens) · default",
        ThinkingLevel::Xhigh => "Extra-high reasoning (~32k tokens)",
        ThinkingLevel::Max => "Maximum reasoning",
    }
}

fn thinking_row(theme: UiTheme, level: ThinkingLevel, supported: bool, selected: bool) -> AnyElement<'static> {
    let prefix = list_selection_row_prefix(selected);
    let (name_color, name_weight) = dialog_option_name_style(theme, selected);
    let row_surface = dialog_row_surface(theme, selected);
    let hint = if supported {
        thinking_level_hint(level)
    } else {
        "Not supported by active model"
    };
    element! {
        View(
            width: 100pct,
            height: 1u16,
            background_color: row_surface,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            justify_content: JustifyContent::FlexStart,
            flex_shrink: 0f32,
            overflow: Overflow::Hidden,
        ) {
            Text(
                content: format!("{prefix}{:<12}", level.label()),
                color: name_color,
                weight: name_weight,
                wrap: TextWrap::NoWrap,
                align: TextAlign::Left,
            )
            Text(
                content: hint,
                color: theme.text_hint,
                wrap: TextWrap::NoWrap,
                align: TextAlign::Left,
            )
        }
    }
    .into()
}

#[component]
pub fn ThinkingSelectorBar(props: &mut ThinkingSelectorBarProps, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);

    if props.pending.is_none() {
        return element! { View() }.into_any();
    }
    let pending = props.pending.as_ref().expect("checked");
    let selected = props.selected_index.map(|s| s.get()).unwrap_or(0);

    let rows: Vec<AnyElement<'static>> = pending
        .rows
        .iter()
        .enumerate()
        .map(|(i, (level, supported))| thinking_row(theme, *level, *supported, i == selected))
        .collect();

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: "Thinking Level".to_string(),
            has_focus: props.has_focus,
            footer_hint: Some(FOOTER_HINT.to_string()),
        ) {
            View(
                width: body_width,
                padding_top: OPTIONS_LIST_TOP_GAP,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(rows)
            }
        }
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_level_hints_match_picker_copy() {
        assert_eq!(thinking_level_hint(ThinkingLevel::Off), "No reasoning");
        assert_eq!(thinking_level_hint(ThinkingLevel::Minimal), "Very brief reasoning (~1k tokens)");
        assert_eq!(thinking_level_hint(ThinkingLevel::Low), "Light reasoning (~2k tokens)");
        assert_eq!(thinking_level_hint(ThinkingLevel::Medium), "Moderate reasoning (~8k tokens)");
        assert_eq!(
            thinking_level_hint(ThinkingLevel::High),
            "Deep reasoning (~16k tokens) · default"
        );
        assert_eq!(thinking_level_hint(ThinkingLevel::Xhigh), "Extra-high reasoning (~32k tokens)");
        assert_eq!(thinking_level_hint(ThinkingLevel::Max), "Maximum reasoning");
    }
}
