//! Compact transcript layout for `ask_user_question` tool calls.

use elph_tui::components::UiTheme;
use elph_tui::utils::wrap_text;
use iocraft::prelude::*;
use serde_json::Value;

use crate::tui::theme::TOOL_ARGS_FG;

/// Gap between the question label column and the answer-hint column.
pub const ASK_USER_LABEL_HINT_GAP: u16 = 2;

/// Minimum hint column width when the terminal is narrow.
pub const ASK_USER_HINT_MIN_COLS: u16 = 10;

const LABEL_COL_MIN: usize = 10;
const LABEL_COL_MAX: usize = 40;
const MAX_HINT_LINES: usize = 2;
const MAX_OPTION_LABELS: usize = 4;
const MAX_VISIBLE_STEPS: usize = 6;

/// One rendered row: question label + answer-type hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskUserToolRow {
    pub label: String,
    pub hint: String,
}

/// Parse ask-user tool args into compact display rows (`None` when not ask-user JSON).
pub fn parse_ask_user_tool_rows(raw: &str) -> Option<Vec<AskUserToolRow>> {
    let value = serde_json::from_str::<Value>(raw.trim()).ok()?;
    if !value.is_object() {
        return None;
    }
    let steps = question_steps_from_value(&value)?;
    if steps.is_empty() {
        return None;
    }
    let step_count = steps.len();
    let mut rows: Vec<AskUserToolRow> = steps
        .iter()
        .enumerate()
        .take(MAX_VISIBLE_STEPS)
        .map(|(index, step)| AskUserToolRow {
            label: step_label(step, index, step_count),
            hint: step_hint(step),
        })
        .collect();
    if step_count > MAX_VISIBLE_STEPS {
        rows.push(AskUserToolRow {
            label: "…".to_string(),
            hint: format!("+{} more", step_count - MAX_VISIBLE_STEPS),
        });
    }
    Some(rows)
}

/// Flat text for transcript scroll sizing (one terminal line per wrapped segment).
pub fn format_ask_user_tool_layout_text(raw: &str) -> String {
    let Some(rows) = parse_ask_user_tool_rows(raw) else {
        return String::new();
    };
    rows.iter()
        .map(|row| {
            if row.hint.is_empty() {
                row.label.clone()
            } else {
                format!("{}  {}", row.label, row.hint)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn question_steps_from_value(value: &Value) -> Option<Vec<&Value>> {
    if let Some(items) = value.get("questions").and_then(Value::as_array) {
        return (!items.is_empty()).then(|| items.iter().collect());
    }
    if let Some(items) = value.get("steps").and_then(Value::as_array) {
        return (!items.is_empty()).then(|| items.iter().collect());
    }
    if let Some(questions) = value.get("questions").filter(|questions| questions.is_object()) {
        return Some(vec![questions]);
    }
    for key in ["question", "prompt", "message", "text"] {
        if value
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty())
        {
            return Some(vec![value]);
        }
    }
    None
}

fn step_label(step: &Value, index: usize, step_count: usize) -> String {
    let question = step
        .get("question")
        .or_else(|| step.get("prompt"))
        .and_then(Value::as_str)
        .unwrap_or("Question");
    let prompt = question.lines().next().unwrap_or(question).trim();
    if step_count > 1 {
        format!("{}/{} · {prompt}", index + 1, step_count)
    } else {
        prompt.to_string()
    }
}

fn step_hint(step: &Value) -> String {
    if let Some(options) = step.get("options").and_then(Value::as_array)
        && !options.is_empty()
    {
        let labels: Vec<String> = options
            .iter()
            .filter_map(option_label)
            .take(MAX_OPTION_LABELS)
            .collect();
        let mut hint = labels.join(", ");
        if options.len() > MAX_OPTION_LABELS {
            hint.push_str(", …");
        }
        let flags = step_mode_flags(step);
        if !flags.is_empty() {
            hint.push_str(" · ");
            hint.push_str(&flags.join(" · "));
        }
        return hint;
    }

    if is_confirm_step(step) {
        return "yes / no".to_string();
    }

    let mut hint = String::from("text");
    if step.get("required").and_then(Value::as_bool) == Some(false) {
        hint.push_str(" · optional");
    }
    if step.get("min_length").and_then(Value::as_u64).is_some() || step.get("pattern").and_then(Value::as_str).is_some()
    {
        hint.push_str(" · validated");
    }
    hint
}

fn option_label(item: &Value) -> Option<String> {
    let label = item.get("label").and_then(Value::as_str);
    let value = item.get("value").and_then(Value::as_str);
    Some(label.or(value)?.to_string())
}

fn is_confirm_step(step: &Value) -> bool {
    step.get("options")
        .and_then(Value::as_array)
        .is_none_or(|items| items.is_empty())
        && step.get("default").and_then(Value::as_bool).is_some()
}

fn step_mode_flags(step: &Value) -> Vec<String> {
    let mut flags = Vec::new();
    if step.get("allow_multiple").and_then(Value::as_bool) == Some(true) {
        flags.push("multi".to_string());
    }
    if step.get("allow_custom").and_then(Value::as_bool) == Some(true) {
        flags.push("custom".to_string());
    }
    if step.get("required").and_then(Value::as_bool) == Some(false) {
        flags.push("optional".to_string());
    }
    flags
}

fn label_column_width(rows: &[AskUserToolRow], list_width: u16) -> u16 {
    let mut max_label = LABEL_COL_MIN;
    for row in rows {
        max_label = max_label.max(row.label.chars().count());
    }
    max_label = max_label.min(LABEL_COL_MAX);

    let max_allowed = list_width
        .saturating_sub(ASK_USER_LABEL_HINT_GAP + ASK_USER_HINT_MIN_COLS)
        .max(1) as usize;
    max_label.min(max_allowed).max(1) as u16
}

fn hint_column_width(list_width: u16, label_width: u16) -> usize {
    list_width.saturating_sub(label_width + ASK_USER_LABEL_HINT_GAP).max(1) as usize
}

fn wrap_hint(hint: &str, list_width: u16, label_width: u16) -> Vec<String> {
    let width = hint_column_width(list_width, label_width);
    let mut lines = wrap_text(hint, width);
    if lines.len() > MAX_HINT_LINES {
        lines.truncate(MAX_HINT_LINES);
        if let Some(last) = lines.last_mut() {
            *last = truncate_ellipsis(last, width);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn truncate_ellipsis(line: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = line.chars().count();
    if char_count <= max_chars {
        return line.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = line.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Props for [`AskUserToolCardView`].
#[derive(Clone, Props)]
pub struct AskUserToolCardViewProps {
    pub width: u16,
    pub raw: String,
    pub label_color: Color,
    pub hint_color: Color,
}

impl Default for AskUserToolCardViewProps {
    fn default() -> Self {
        let theme = UiTheme::default();
        Self {
            width: 40,
            raw: String::new(),
            label_color: TOOL_ARGS_FG,
            hint_color: theme.text_muted,
        }
    }
}

/// Slash-palette-style rows: question label beside answer hint.
#[component]
pub fn AskUserToolCardView(props: &AskUserToolCardViewProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let Some(rows) = parse_ask_user_tool_rows(&props.raw) else {
        return element! { View(width: props.width) };
    };
    if rows.is_empty() {
        return element! { View(width: props.width) };
    }

    let label_width = label_column_width(&rows, props.width);
    let hint_width = hint_column_width(props.width, label_width);
    let mut elements: Vec<AnyElement<'static>> = Vec::new();

    for row in &rows {
        let hint_lines = wrap_hint(&row.hint, props.width, label_width);
        let row_height = hint_lines.len().max(1) as u16;
        let hint_text = hint_lines.join("\n");
        elements.push(
            element! {
                View(
                    width: props.width,
                    height: row_height,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexStart,
                    gap: ASK_USER_LABEL_HINT_GAP,
                    flex_shrink: 0f32,
                ) {
                    View(width: label_width, height: row_height, align_items: AlignItems::FlexStart) {
                        Text(
                            content: row.label.clone(),
                            color: props.label_color,
                            wrap: TextWrap::NoWrap,
                        )
                    }
                    View(width: hint_width as u16, height: row_height, align_items: AlignItems::FlexStart) {
                        Text(
                            content: hint_text,
                            color: props.hint_color,
                            wrap: TextWrap::NoWrap,
                        )
                    }
                }
            }
            .into(),
        );
    }

    element! {
        View(
            width: props.width,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            #(elements)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_step_with_hints() {
        let raw = r#"{
            "questions": [
                { "question": "Pick a color", "options": [{ "value": "red", "label": "Red" }, { "value": "blue", "label": "Blue" }], "allow_custom": true },
                { "question": "Any notes?" }
            ]
        }"#;
        let rows = parse_ask_user_tool_rows(raw).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "1/2 · Pick a color");
        assert_eq!(rows[0].hint, "Red, Blue · custom");
        assert_eq!(rows[1].label, "2/2 · Any notes?");
        assert_eq!(rows[1].hint, "text");
    }

    #[test]
    fn legacy_single_question_parses() {
        let raw = r#"{
            "question": "Proceed?",
            "options": [{ "value": "y", "label": "Yes" }, { "value": "n", "label": "No" }]
        }"#;
        let rows = parse_ask_user_tool_rows(raw).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Proceed?");
        assert_eq!(rows[0].hint, "Yes, No");
    }

    #[test]
    fn confirm_step_shows_yes_no_hint() {
        let raw = r#"{ "question": "Delete file?", "default": true }"#;
        let rows = parse_ask_user_tool_rows(raw).unwrap();
        assert_eq!(rows[0].hint, "yes / no");
    }

    #[test]
    fn label_column_shrinks_on_narrow_width() {
        let rows = vec![
            AskUserToolRow {
                label: "1/3 · Very long question title".to_string(),
                hint: "a".into(),
            },
            AskUserToolRow {
                label: "2/3 · Short".to_string(),
                hint: "b".into(),
            },
        ];
        let wide = label_column_width(&rows, 80);
        let narrow = label_column_width(&rows, 24);
        assert!(narrow < wide);
        assert!(hint_column_width(24, narrow) >= ASK_USER_HINT_MIN_COLS as usize);
    }

    #[test]
    fn long_hints_wrap_on_narrow_terminal() {
        let raw = r#"{
            "question": "Tags",
            "options": [
                { "value": "a", "label": "Alpha" },
                { "value": "b", "label": "Beta" },
                { "value": "c", "label": "Gamma" },
                { "value": "d", "label": "Delta" },
                { "value": "e", "label": "Epsilon" }
            ],
            "allow_multiple": true
        }"#;
        let rows = parse_ask_user_tool_rows(raw).unwrap();
        let label_width = label_column_width(&rows, 32);
        let wrapped = wrap_hint(&rows[0].hint, 32, label_width);
        assert!(!wrapped.is_empty());
    }
}
