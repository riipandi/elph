//! Read-only checklist with optional detail lines.

use crate::components::theme::{UiTheme, resolve_ui_theme};
use crate::types::{DialogTodoItem, DialogTodoStatus};
use iocraft::prelude::*;

/// Prefix glyph for a todo row.
pub fn todo_row_prefix(status: DialogTodoStatus) -> &'static str {
    match status {
        DialogTodoStatus::Pending => "☐",
        DialogTodoStatus::InProgress => "◐",
        DialogTodoStatus::Done => "☑",
        DialogTodoStatus::Skipped => "⊘",
    }
}

/// Primary line for one todo item.
pub fn todo_row_line(item: &DialogTodoItem) -> String {
    format!("{} {}", todo_row_prefix(item.status), item.label)
}

/// Props for [`DialogTodoListContent`].
#[derive(Clone, Props)]
pub struct DialogTodoListContentProps {
    pub width: u16,
    pub items: Vec<DialogTodoItem>,
    pub done_color: Color,
    pub pending_color: Color,
    pub in_progress_color: Color,
    pub skipped_color: Color,
    pub detail_color: Color,
    pub theme: Option<UiTheme>,
}

impl Default for DialogTodoListContentProps {
    fn default() -> Self {
        let theme = UiTheme::default();
        Self {
            width: 40,
            items: Vec::new(),
            done_color: theme.success,
            pending_color: theme.text_secondary,
            in_progress_color: theme.warning,
            skipped_color: theme.text_muted,
            detail_color: theme.text_hint,
            theme: None,
        }
    }
}

/// Read-only checklist with optional detail lines.
#[component]
pub fn DialogTodoListContent(props: &DialogTodoListContentProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let row_gap = theme.dialog_row_gap();

    let rows: Vec<_> = props
        .items
        .iter()
        .flat_map(|item| {
            let color = match item.status {
                DialogTodoStatus::Done => props.done_color,
                DialogTodoStatus::Skipped => props.skipped_color,
                DialogTodoStatus::Pending => props.pending_color,
                DialogTodoStatus::InProgress => props.in_progress_color,
            };
            let mut elements: Vec<AnyElement<'static>> = vec![
                element! {
                    Text(
                        content: todo_row_line(item),
                        color: color,
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            ];
            if !item.detail.is_empty() {
                elements.push(
                    element! {
                        View(padding_left: theme.detail_padding_left()) {
                            Text(
                                content: item.detail.clone(),
                                color: props.detail_color,
                                wrap: TextWrap::Wrap,
                            )
                        }
                    }
                    .into(),
                );
            }
            elements
        })
        .collect();

    element! {
        View(
            width: props.width,
            flex_direction: FlexDirection::Column,
            gap: row_gap,
            flex_shrink: 0f32,
        ) {
            #(rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_reflect_status() {
        assert_eq!(todo_row_prefix(DialogTodoStatus::Pending), "☐");
        assert_eq!(todo_row_prefix(DialogTodoStatus::InProgress), "◐");
        assert_eq!(todo_row_prefix(DialogTodoStatus::Done), "☑");
        assert_eq!(todo_row_prefix(DialogTodoStatus::Skipped), "⊘");
    }
}
