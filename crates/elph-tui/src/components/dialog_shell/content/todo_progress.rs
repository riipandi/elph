//! In-flight todo progress dialog body + live shell todo panel.
//!
//! [`DialogTodoProgressContent`] is the static dialog preset. [`TodoProgressPanel`]
//! is the compact live panel used by the coding-agent shell above the status row:
//! per-status glyph (`○` pending / animated spinner `◆` running / `✓` finished),
//! a plain-language status word (never color alone), header with
//! `Todos · done/total · ↑hidden`, configurable row cap with a `↓N more` hint,
//! and finished rows hidden from the visible list.

use super::layout::dialog_body_row_gap;
use crate::components::status_indicator::{
    GLYPH_DONE, GLYPH_QUEUED, GLYPH_RUNNING, ProcessStatus, ProcessStatusRow, process_status_glyph,
};
use crate::components::theme::{UiTheme, resolve_ui_theme};
use crate::types::{DialogTodoProgress, DialogTodoProgressItem};
use iocraft::prelude::*;

/// Live `pending` glyph — hollow circle (not a running row).
pub const TODO_GLYPH_PENDING: &str = GLYPH_QUEUED; // ○
/// Live `in_progress` glyph — hollow dotted circle (spinner when `tick` animates).
pub const TODO_GLYPH_RUNNING: &str = GLYPH_RUNNING; // ◌
/// Live `completed` / `cancelled` glyph.
pub const TODO_GLYPH_DONE: &str = GLYPH_DONE; // ✓

/// Default max visible rows for [`TodoProgressPanel`] (before `↓N more`).
pub const TODO_PANEL_DEFAULT_MAX_ROWS: usize = 6;

/// Plain-language status word for a live todo row (a11y — never color alone).
pub fn todo_status_word(finished: bool, running: bool) -> &'static str {
    if running {
        "running"
    } else if finished {
        "done"
    } else {
        "pending"
    }
}

/// Status glyph for a live todo row (caption: text is asymmetric, so never rely on color).
pub fn todo_status_glyph(running: bool, finished: bool) -> &'static str {
    if running {
        TODO_GLYPH_RUNNING
    } else if finished {
        TODO_GLYPH_DONE
    } else {
        TODO_GLYPH_PENDING
    }
}

/// Single display row for a live todo item.
#[derive(Debug, Clone)]
pub struct TodoPanelRow {
    pub label: String,
    pub running: bool,
    pub finished: bool,
}

/// Build display rows from the raw todo list: finished rows are hidden from the
/// visible list, pending/in-progress stay in order. Counts are always full-list.
pub fn build_todo_panel_rows(todos: &[TodoPanelRow]) -> (Vec<TodoPanelRow>, usize, usize, usize) {
    let total = todos.len();
    let done = todos.iter().filter(|t| t.finished).count();
    let visible: Vec<TodoPanelRow> = todos.iter().filter(|t| !t.finished).cloned().collect();
    let visible_count = visible.len();
    (visible, visible_count, done, total)
}

/// Header line: `Todos · 2/5 · ↑1`.
pub fn todo_panel_header_line(visible_count: usize, done: usize, total: usize) -> String {
    let hidden = total.saturating_sub(visible_count);
    let mut line = format!("{TODO_PANEL_HEADER_PREFIX} {done}/{total}");
    if hidden > 0 {
        line.push_str(&format!(" · ↑{hidden}"));
    }
    line
}

/// Header label shown above the live todo rows.
pub const TODO_PANEL_HEADER_PREFIX: &str = "Todos";

/// Props for [`TodoProgressPanel`].
#[derive(Clone, Props)]
pub struct TodoProgressPanelProps {
    pub width: u16,
    pub items: Vec<TodoPanelRow>,
    /// Optional tick counter — when non-zero + a running row exists, running rows
    /// render an animated braille spinner instead of the static `◌`.
    pub tick: u32,
    /// Max visible rows before the `↓N more` hint (0 = show all).
    pub max_rows: usize,
    pub theme: Option<UiTheme>,
}

impl Default for TodoProgressPanelProps {
    fn default() -> Self {
        Self {
            width: 40,
            items: Vec::new(),
            tick: 0,
            max_rows: TODO_PANEL_DEFAULT_MAX_ROWS,
            theme: None,
        }
    }
}

#[component]
pub fn TodoProgressPanel(props: &TodoProgressPanelProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let (visible, visible_count, done, total) = build_todo_panel_rows(&props.items);
    let cap = if props.max_rows == 0 {
        visible_count
    } else {
        props.max_rows.min(visible_count)
    };
    let show_more = visible_count > cap;
    let header = todo_panel_header_line(visible_count, done, total);
    let animate = props.tick != 0;
    let spinner_glyph = spinner_glyph_for_tick(props.tick);

    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    for row in visible.iter().take(cap) {
        let color = if row.running {
            theme.warning
        } else {
            theme.text_secondary
        };
        let glyph = if row.running && animate {
            spinner_glyph
        } else {
            todo_status_glyph(row.running, row.finished)
        };
        rows.push(
            element! {
                View(
                    flex_direction: FlexDirection::Row,
                    gap: theme.gap_md,
                    align_items: AlignItems::Center,
                    flex_shrink: 0f32,
                ) {
                    View(flex_shrink: 0f32) {
                        Text(content: glyph.to_string(), color: color, wrap: TextWrap::NoWrap)
                    }
                    Text(
                        content: row.label.clone(),
                        color: color,
                        weight: if row.running { Weight::Bold } else { Weight::Normal },
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            .into(),
        );
    }
    if show_more {
        rows.push(
            element! {
                Text(
                    content: format!("  ↓{} more", visible_count.saturating_sub(cap)),
                    color: theme.text_hint,
                    wrap: TextWrap::NoWrap,
                )
            }
            .into(),
        );
    }

    element! {
        View(
            width: props.width,
            flex_shrink: 0f32,
            flex_direction: FlexDirection::Column,
        ) {
            View(width: props.width, flex_shrink: 0f32) {
                Text(content: header, color: theme.text_hint, wrap: TextWrap::NoWrap)
            }
            View(width: props.width, flex_direction: FlexDirection::Column, gap: theme.gap_md, flex_shrink: 0f32) {
                #(rows)
            }
        }
    }
}

/// Braille spinner frame for a polled tick (wall-clock phase, skips when lagging).
fn spinner_glyph_for_tick(tick: u32) -> &'static str {
    crate::loader::SpinnerLoader::glyph_for_elapsed_ms((tick as u64) * 80)
}

// ---------------------------------------------------------------------------
// Static dialog preset (unchanged API)
// ---------------------------------------------------------------------------

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

    #[test]
    fn live_glyphs_and_words() {
        assert_eq!(todo_status_glyph(false, false), "○");
        assert_eq!(todo_status_glyph(true, false), "◌");
        assert_eq!(todo_status_glyph(false, true), "✓");
        assert_eq!(todo_status_word(false, false), "pending");
        assert_eq!(todo_status_word(false, true), "running");
        assert_eq!(todo_status_word(true, false), "done");
    }

    #[test]
    fn finished_rows_hidden_from_visible_list() {
        let rows = vec![
            TodoPanelRow {
                label: "a".into(),
                running: false,
                finished: false,
            },
            TodoPanelRow {
                label: "b".into(),
                running: true,
                finished: false,
            },
            TodoPanelRow {
                label: "c".into(),
                running: false,
                finished: true,
            },
            TodoPanelRow {
                label: "d".into(),
                running: false,
                finished: true,
            },
        ];
        let (visible, visible_count, done, total) = build_todo_panel_rows(&rows);
        assert_eq!(visible_count, 2);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].label, "a");
        assert_eq!(visible[1].label, "b");
        assert_eq!(done, 2);
        assert_eq!(total, 4);
    }

    #[test]
    fn header_reports_counts_and_hidden() {
        assert_eq!(todo_panel_header_line(2, 1, 3), "Todos 1/3 · ↑1");
        assert_eq!(todo_panel_header_line(3, 0, 3), "Todos 0/3");
    }

    #[test]
    fn row_cap_controls_visible_rows() {
        let rows: Vec<TodoPanelRow> = (0..8)
            .map(|i| TodoPanelRow {
                label: format!("t{i}"),
                running: false,
                finished: false,
            })
            .collect();
        let (visible, visible_count, _, _) = build_todo_panel_rows(&rows);
        assert_eq!(visible_count, 8);
        assert_eq!(visible.len(), 8);
        // cap = max_rows, not total (6 of 8 visible, 2 hidden by hint)
        assert_eq!(visible_count.saturating_sub(TODO_PANEL_DEFAULT_MAX_ROWS), 2);
    }
}
