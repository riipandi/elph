//! Model list rows — compact name column + tabular hint column.

use elph_tui::components::theme::{UiTheme, dialog_option_desc_style, dialog_option_name_style, dialog_row_surface};
use elph_tui::list_selection_row_prefix;
use iocraft::prelude::*;

use crate::tui::model_selector::ModelRow;
use crate::tui::slash_palette::palette_window_start;

/// Selection marker width (`❯ ` or `  `).
const ROW_PREFIX_CHARS: usize = 2;

/// Gap between model name and hint (tighter than slash palette).
pub const MODEL_NAME_HINT_GAP: u16 = 1;

/// Two spaces between aligned hint columns.
const HINT_COL_GAP: &str = "  ";

const MODEL_NAME_MIN_CHARS: usize = 10;
const MODEL_NAME_MAX_CHARS: usize = 30;

/// Viewport height and visible row count for a fixed-height model list.
pub fn model_option_list_viewport(height: u16, option_count: usize) -> (u16, usize) {
    let viewport_height = if height == 0 {
        option_count.max(1) as u16
    } else {
        height
    };
    let scroll_cap = if option_count == 0 {
        1
    } else {
        (viewport_height as usize).min(option_count)
    };
    (viewport_height, scroll_cap)
}

fn model_name_label_width(name: &str) -> usize {
    ROW_PREFIX_CHARS.saturating_add(name.chars().count())
}

pub fn model_name_column_width(models: &[ModelRow], list_width: u16) -> u16 {
    let mut max_label = MODEL_NAME_MIN_CHARS;
    for row in models {
        max_label = max_label.max(model_name_label_width(&row.name));
    }
    max_label = max_label.min(MODEL_NAME_MAX_CHARS);

    let max_allowed = list_width.saturating_sub(MODEL_NAME_HINT_GAP + 8).max(1) as usize;
    max_label.min(max_allowed).max(1) as u16
}

fn model_hint_desc_width(list_width: u16, name_col: u16) -> usize {
    list_width.saturating_sub(name_col + MODEL_NAME_HINT_GAP).max(1) as usize
}

/// Build single-line tabular hints with aligned provider / model id columns.
pub fn format_model_hints_tabular(models: &[ModelRow], show_provider: bool) -> Vec<String> {
    if models.is_empty() {
        return Vec::new();
    }

    let provider_w = if show_provider {
        models
            .iter()
            .map(|row| row.provider_id.chars().count())
            .max()
            .unwrap_or(0)
    } else {
        0
    };
    let model_w = models.iter().map(|row| row.model_id.chars().count()).max().unwrap_or(0);

    models
        .iter()
        .map(|row| {
            let mut parts = Vec::new();
            if show_provider {
                parts.push(format!("{:<provider_w$}", row.provider_id, provider_w = provider_w));
            }
            parts.push(format!("{:<model_w$}", row.model_id, model_w = model_w));
            parts.push(format!("{}k", row.context_k));
            if row.reasoning {
                parts.push("think".to_string());
            }
            parts.join(HINT_COL_GAP)
        })
        .collect()
}

fn truncate_hint(hint: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = hint.chars().count();
    if char_count <= max_chars {
        return hint.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = hint.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn model_row(
    theme: UiTheme,
    list_width: u16,
    name_col: u16,
    name: &str,
    hint: &str,
    selected: bool,
) -> AnyElement<'static> {
    let prefix = list_selection_row_prefix(selected);
    let (name_color, name_weight) = dialog_option_name_style(theme, selected);
    let desc_color = dialog_option_desc_style(theme, selected);
    let desc_width = model_hint_desc_width(list_width, name_col);
    let hint_text = truncate_hint(hint, desc_width);
    let row_surface = dialog_row_surface(theme, selected);

    element! {
        View(
            width: list_width,
            height: 1,
            background_color: row_surface,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            justify_content: JustifyContent::FlexStart,
            gap: MODEL_NAME_HINT_GAP,
            flex_shrink: 0f32,
        ) {
            View(width: name_col, height: 1, align_items: AlignItems::FlexStart) {
                Text(
                    content: format!("{prefix}{name}"),
                    color: name_color,
                    weight: name_weight,
                    wrap: TextWrap::NoWrap,
                    align: TextAlign::Left,
                )
            }
            View(width: desc_width as u16, height: 1, align_items: AlignItems::FlexStart) {
                Text(
                    content: hint_text,
                    color: desc_color,
                    wrap: TextWrap::NoWrap,
                    align: TextAlign::Left,
                )
            }
        }
    }
    .into()
}

#[derive(Default, Props)]
pub struct ModelOptionListProps {
    pub width: u16,
    pub height: u16,
    pub models: Vec<ModelRow>,
    pub show_provider_hint: bool,
    pub selected_index: Option<State<usize>>,
    pub has_focus: bool,
    pub theme: Option<UiTheme>,
}

#[component]
pub fn ModelOptionList(props: &mut ModelOptionListProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = props.theme.unwrap_or_default();
    let internal_index = hooks.use_state(|| 0usize);
    let mut selected = props.selected_index.unwrap_or(internal_index);
    let has_focus = props.has_focus;
    let models = props.models.clone();
    let show_provider_hint = props.show_provider_hint;
    let option_count = models.len();

    hooks.use_terminal_events(move |event| {
        if !has_focus || option_count == 0 {
            return;
        }
        let TerminalEvent::Key(KeyEvent {
            code, kind, modifiers, ..
        }) = event
        else {
            return;
        };
        if kind == KeyEventKind::Release {
            return;
        }
        if modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT) {
            return;
        }
        let prev = selected.get();
        let next = match code {
            KeyCode::Up | KeyCode::Char('k') => prev.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => (prev + 1).min(option_count.saturating_sub(1)),
            _ => prev,
        };
        if next != prev {
            selected.set(next);
        }
    });

    let index = if option_count == 0 {
        0
    } else {
        selected.get().min(option_count.saturating_sub(1))
    };

    let (viewport_height, scroll_cap) = model_option_list_viewport(props.height, option_count);
    let window_start = palette_window_start(index, scroll_cap, option_count);
    let name_col = model_name_column_width(&models, props.width);
    let hints = format_model_hints_tabular(&models, show_provider_hint);

    let rows: Vec<AnyElement<'static>> = if models.is_empty() {
        vec![
            element! {
                Text(content: "(no models)".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
            }
            .into(),
        ]
    } else {
        models
            .iter()
            .zip(hints.iter())
            .enumerate()
            .skip(window_start)
            .take(scroll_cap)
            .map(|(i, (row, hint))| model_row(theme, props.width, name_col, &row.name, hint, i == index))
            .collect()
    };

    element! {
        View(
            width: props.width,
            height: viewport_height.max(1),
            flex_direction: FlexDirection::Column,
            gap: 0,
            overflow: Overflow::Hidden,
            flex_shrink: 0f32,
        ) {
            #(rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::model_selector::ModelRow;

    fn sample_row(provider: &str, model_id: &str, name: &str, context_k: u32, reasoning: bool) -> ModelRow {
        ModelRow {
            value: format!("{provider}/{model_id}"),
            name: name.to_string(),
            provider_id: provider.to_string(),
            model_id: model_id.to_string(),
            context_k,
            reasoning,
        }
    }

    #[test]
    fn fixed_viewport_keeps_container_height_with_few_options() {
        let (height, scroll_cap) = model_option_list_viewport(8, 2);
        assert_eq!(height, 8);
        assert_eq!(scroll_cap, 2);
    }

    #[test]
    fn fixed_viewport_scrolls_when_options_exceed_height() {
        let (height, scroll_cap) = model_option_list_viewport(8, 20);
        assert_eq!(height, 8);
        assert_eq!(scroll_cap, 8);
    }

    #[test]
    fn auto_height_grows_with_option_count() {
        let (height, scroll_cap) = model_option_list_viewport(0, 3);
        assert_eq!(height, 3);
        assert_eq!(scroll_cap, 3);
    }

    #[test]
    fn tabular_hints_align_provider_and_model_columns() {
        let rows = vec![
            sample_row("anthropic", "claude-sonnet-4", "Claude Sonnet 4", 200, false),
            sample_row("openai", "gpt-4.1", "GPT-4.1", 128, true),
        ];
        let hints = format_model_hints_tabular(&rows, true);
        assert_eq!(hints[0], "anthropic  claude-sonnet-4  200k");
        assert_eq!(hints[1], "openai     gpt-4.1          128k  think");
        assert!(hints[0].contains("claude-sonnet-4"));
        assert!(hints[1].contains("gpt-4.1"));
    }

    #[test]
    fn provider_tab_hints_omit_provider_column() {
        let rows = vec![sample_row("anthropic", "claude-opus-4", "Claude Opus 4", 200, true)];
        let hints = format_model_hints_tabular(&rows, false);
        assert_eq!(hints[0], "claude-opus-4  200k  think");
    }

    #[test]
    fn name_hint_gap_is_tighter_than_slash_palette() {
        assert_eq!(MODEL_NAME_HINT_GAP, 1);
    }
}
