//! In-flight todo progress dialog body.

use super::layout::dialog_body_row_gap;
use crate::components::status_indicator::{ProcessStatus, ProcessStatusRow, process_status_glyph};
use crate::components::theme::{UiTheme, resolve_ui_theme};
use crate::types::{DialogTodoProgress, DialogTodoProgressItem};
use iocraft::prelude::*;

/// Static glyph for a progress row (except running, which uses a spinner).
pub fn progress_row_glyph(state: DialogTodoProgress) -> &'static str {
    process_status_glyph(ProcessStatus::from(state))
}

/// Props for [`DialogTodoProgressContent`].
#[derive(Clone, Props)]
pub struct DialogTodoProgressContentProps {
    pub width: u16,
    pub items: Vec<DialogTodoProgressItem>,
    pub queued_color: Color,
    pub running_color: Color,
    pub done_color: Color,
    pub failed_color: Color,
    pub theme: Option<UiTheme>,
}

impl Default for DialogTodoProgressContentProps {
    fn default() -> Self {
        let theme = UiTheme::default();
        Self {
            width: 40,
            items: Vec::new(),
            queued_color: theme.text_muted,
            running_color: theme.warning,
            done_color: theme.success,
            failed_color: theme.error,
            theme: None,
        }
    }
}

/// Todo list with animated spinner on the active row.
#[component]
pub fn DialogTodoProgressContent(
    props: &DialogTodoProgressContentProps,
    hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let row_gap = dialog_body_row_gap(theme);

    let rows: Vec<_> = props
        .items
        .iter()
        .map(|item| {
            element! {
                ProcessStatusRow(
                    status: ProcessStatus::from(item.state),
                    label: item.label.clone(),
                    queued_color: Some(props.queued_color),
                    running_color: Some(props.running_color),
                    done_color: Some(props.done_color),
                    failed_color: Some(props.failed_color),
                    theme: props.theme,
                    emphasize_running: true,
                )
            }
            .into()
        })
        .collect::<Vec<AnyElement<'static>>>();

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
    fn glyphs_match_state() {
        assert_eq!(progress_row_glyph(DialogTodoProgress::Queued), "○");
        assert_eq!(progress_row_glyph(DialogTodoProgress::Done), "✓");
        assert_eq!(progress_row_glyph(DialogTodoProgress::Failed), "✗");
    }
}
