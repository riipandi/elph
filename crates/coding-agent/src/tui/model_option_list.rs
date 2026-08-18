//! Model list rows — model id column + tabular hint (provider / context / caps).

use elph_tui::components::theme::{UiTheme, dialog_option_desc_style, dialog_option_name_style, dialog_row_surface};
use elph_tui::list_selection_row_prefix;
use iocraft::prelude::*;

use crate::tui::model_selector::{ModelRow, format_model_capability_label, format_model_context_label};
use crate::tui::slash_palette::palette_window_start;

/// Selection marker width (`❯ ` or `  `).
const ROW_PREFIX_CHARS: usize = 2;

/// Gap between model id column and hint columns (matches slash palette).
pub const MODEL_ID_HINT_GAP: u16 = 2;

/// Two spaces between aligned hint columns.
const HINT_COL_GAP: &str = "  ";

const MODEL_ID_MIN_CHARS: usize = 12;
/// Fits long provider-style ids (e.g. Bedrock `au.anthropic.claude-…-v1`).
const MODEL_ID_MAX_CHARS: usize = 52;

/// Reserve enough room for `provider  128K  (think|img)` so the id column cannot crowd it.
const MIN_HINT_CHARS: u16 = 28;

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

fn model_id_label_width(model_id: &str) -> usize {
    ROW_PREFIX_CHARS.saturating_add(model_id.chars().count())
}

pub fn model_id_column_width(models: &[ModelRow], list_width: u16) -> u16 {
    let mut max_label = MODEL_ID_MIN_CHARS;
    for row in models {
        max_label = max_label.max(model_id_label_width(&row.model_id));
    }
    max_label = max_label.min(MODEL_ID_MAX_CHARS);

    let max_allowed = list_width.saturating_sub(MODEL_ID_HINT_GAP + MIN_HINT_CHARS).max(1) as usize;
    max_label.min(max_allowed).max(1) as u16
}

fn model_hint_desc_width(list_width: u16, id_col: u16) -> usize {
    list_width.saturating_sub(id_col + MODEL_ID_HINT_GAP).max(1) as usize
}

/// Truncate a string to `max_chars`, appending `…` when clipped.
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Build single-line tabular hints: `PROVIDER  CONTEXT (think|img)`.
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
    let context_labels: Vec<String> = models
        .iter()
        .map(|row| format_model_context_label(row.context_k))
        .collect();
    let context_w = context_labels
        .iter()
        .map(|label| label.chars().count())
        .max()
        .unwrap_or(0);

    models
        .iter()
        .zip(context_labels.iter())
        .map(|(row, context)| {
            let mut parts = Vec::new();
            if show_provider {
                parts.push(format!("{:<provider_w$}", row.provider_id, provider_w = provider_w));
            }
            parts.push(format!("{:<context_w$}", context, context_w = context_w));
            if let Some(caps) = format_model_capability_label(row.reasoning, row.images) {
                parts.push(caps);
            }
            parts.join(HINT_COL_GAP)
        })
        .collect()
}

fn model_row(
    theme: UiTheme,
    list_width: u16,
    id_col: u16,
    model_id: &str,
    hint: &str,
    selected: bool,
) -> AnyElement<'static> {
    let prefix = list_selection_row_prefix(selected);
    let (id_color, id_weight) = dialog_option_name_style(theme, selected);
    let desc_color = dialog_option_desc_style(theme, selected);
    let desc_width = model_hint_desc_width(list_width, id_col);
    // Keep id text inside the column so long Bedrock-style ids never paint over the provider.
    let id_content_max = (id_col as usize).saturating_sub(ROW_PREFIX_CHARS);
    let display_id = truncate_chars(model_id, id_content_max);
    let hint_text = truncate_chars(hint, desc_width);
    let row_surface = dialog_row_surface(theme, selected);

    element! {
        View(
            width: list_width,
            height: 1,
            background_color: row_surface,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            justify_content: JustifyContent::FlexStart,
            gap: MODEL_ID_HINT_GAP,
            flex_shrink: 0f32,
            overflow: Overflow::Hidden,
        ) {
            View(
                width: id_col,
                height: 1,
                align_items: AlignItems::FlexStart,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
                Text(
                    content: format!("{prefix}{display_id}"),
                    color: id_color,
                    weight: id_weight,
                    wrap: TextWrap::NoWrap,
                    align: TextAlign::Left,
                )
            }
            View(
                width: desc_width as u16,
                height: 1,
                align_items: AlignItems::FlexStart,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
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
    /// Override the computed hint text per row (e.g. provider config status).
    /// When set, must have the same length as `models`.
    pub custom_hints: Vec<String>,
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
            KeyCode::Up => prev.saturating_sub(1),
            KeyCode::Down => (prev + 1).min(option_count.saturating_sub(1)),
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
    let id_col = model_id_column_width(&models, props.width);
    let hints = if !props.custom_hints.is_empty() {
        props.custom_hints.clone()
    } else {
        format_model_hints_tabular(&models, show_provider_hint)
    };

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
            .map(|(i, (row, hint))| model_row(theme, props.width, id_col, &row.model_id, hint, i == index))
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

    fn sample_row(
        provider: &str,
        model_id: &str,
        name: &str,
        context_k: u32,
        reasoning: bool,
        images: bool,
    ) -> ModelRow {
        ModelRow {
            value: format!("{provider}/{model_id}"),
            name: name.to_string(),
            provider_id: provider.to_string(),
            model_id: model_id.to_string(),
            context_k,
            reasoning,
            images,
            is_free: false,
            cost_per_m_input: 0.0,
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
    fn tabular_hints_align_provider_context_and_caps() {
        let rows = vec![
            sample_row("openai", "gpt-5.6-luna", "GPT-5.6 Luna", 128, true, false),
            sample_row("openai", "gpt-5.4", "GPT-5.4", 1000, true, true),
            sample_row("anthropic", "claude-haiku-4-5", "Claude Haiku 4.5", 200, true, false),
        ];
        let hints = format_model_hints_tabular(&rows, true);
        assert_eq!(hints[0], "openai     128K  (think)");
        assert_eq!(hints[1], "openai     1M    (think|img)");
        assert_eq!(hints[2], "anthropic  200K  (think)");
    }

    #[test]
    fn tabular_hints_can_omit_provider_column() {
        let rows = vec![sample_row(
            "anthropic",
            "claude-opus-4",
            "Claude Opus 4",
            200,
            true,
            true,
        )];
        let hints = format_model_hints_tabular(&rows, false);
        assert_eq!(hints[0], "200K  (think|img)");
    }

    #[test]
    fn id_column_width_uses_model_id_not_name() {
        let rows = vec![sample_row(
            "openai",
            "gpt-5.4",
            "A Very Long Display Name That Should Not Drive Width",
            128,
            true,
            false,
        )];
        // prefix (2) + "gpt-5.4" (7) = 9, floored by MODEL_ID_MIN_CHARS.
        assert_eq!(model_id_column_width(&rows, 80), MODEL_ID_MIN_CHARS as u16);
    }

    #[test]
    fn id_column_fits_long_bedrock_style_ids() {
        let rows = vec![sample_row(
            "amazon-bedrock",
            "au.anthropic.claude-opus-4-6-v1",
            "Claude Opus 4.6",
            1000,
            true,
            true,
        )];
        // prefix (2) + id (31) = 33 — well under max, leaves room for provider hint.
        assert_eq!(model_id_column_width(&rows, 100), 33);
        // Narrow terminals still reserve MIN_HINT_CHARS for the provider/context side.
        let narrow = model_id_column_width(&rows, 50);
        assert!(narrow < 33);
        assert!(narrow + MODEL_ID_HINT_GAP + MIN_HINT_CHARS <= 50);
    }

    #[test]
    fn truncate_chars_adds_ellipsis() {
        assert_eq!(truncate_chars("au.anthropic.claude-opus-4-6-v1", 20), "au.anthropic.claude…");
        assert_eq!(truncate_chars("short", 20), "short");
    }

    #[test]
    fn id_hint_gap_matches_slash_palette() {
        assert_eq!(MODEL_ID_HINT_GAP, 2);
        assert_eq!(MODEL_ID_HINT_GAP, 2);
    }
}
