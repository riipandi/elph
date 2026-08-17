//! In-flight todo progress dialog body + live shell todo panel.
//!
//! [`DialogTodoProgressContent`] is the static dialog preset. [`TodoProgressPanel`]
//! is the compact live panel used by the coding-agent shell above the status row:
//! minimal single border with inline title `Todos done/total`, per-status glyph
//! (`○` pending / animated spinner when running / `✓` finished), finished rows
//! hidden from the list, and the whole panel hidden once every item is finished.
//! When the user steers or interjects, rows dim so the plan reads as provisional.

use super::layout::dialog_body_row_gap;
use crate::components::status_indicator::{
    GLYPH_DONE, GLYPH_QUEUED, ProcessStatus, ProcessStatusRow, process_status_glyph,
};
use crate::components::theme::{UiTheme, resolve_ui_theme};
use crate::types::{DialogTodoProgress, DialogTodoProgressItem};
use iocraft::prelude::*;

/// Live `pending` glyph — hollow circle.
pub const TODO_GLYPH_PENDING: &str = GLYPH_QUEUED; // ○
/// Live `in_progress` glyph — half-filled circle (static, clearer than braille spinner).
pub const TODO_GLYPH_RUNNING: &str = "\u{25D0}"; // ◐
/// Live `completed` / `cancelled` glyph — full-filled circle.
pub const TODO_GLYPH_DONE: &str = GLYPH_DONE; // ✓

/// Default max visible rows for [`TodoProgressPanel`] (before `↓N more`).
pub const TODO_PANEL_DEFAULT_MAX_ROWS: usize = 5;

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

/// Build display rows from the raw todo list: all items stay in their original
/// order (including finished ones). Counts are always full-list.
///
/// The panel renderer applies the row cap and hides finished items from the tail
/// when the list exceeds available space.
pub fn build_todo_panel_rows(todos: &[TodoPanelRow]) -> (Vec<TodoPanelRow>, usize, usize) {
    let total = todos.len();
    let done = todos.iter().filter(|t| t.finished).count();
    (todos.to_vec(), done, total)
}

/// Whether the live panel should paint: hide when empty or when every item is finished.
pub fn todo_panel_should_show(todos: &[TodoPanelRow]) -> bool {
    if todos.is_empty() {
        return false;
    }
    // Hide once every item is finished (parent also gates on this).
    !todos.iter().all(|t| t.finished)
}

/// Border title: `Todos 2/5` or `Todos 2/5 · steered` when the user redirected.
pub fn todo_panel_header_line(done: usize, total: usize, redirected: bool) -> String {
    if redirected {
        format!("{TODO_PANEL_HEADER_PREFIX} {done}/{total} · steered")
    } else {
        format!("{TODO_PANEL_HEADER_PREFIX} {done}/{total}")
    }
}

/// Header label fragment inside the border title (without parentheses / counts).
pub const TODO_PANEL_HEADER_PREFIX: &str = "Todos";

/// Props for [`TodoProgressPanel`].
#[derive(Clone, Props)]
pub struct TodoProgressPanelProps {
    pub width: u16,
    pub items: Vec<TodoPanelRow>,
    /// Max visible rows before the `↓N more` hint (0 = show all).
    pub max_rows: usize,
    /// User steered / interjected while this plan is still on screen — dim rows
    /// and annotate the border title so the checklist reads as provisional.
    pub redirected: bool,
    pub theme: Option<UiTheme>,
}

impl Default for TodoProgressPanelProps {
    fn default() -> Self {
        Self {
            width: 40,
            items: Vec::new(),
            max_rows: TODO_PANEL_DEFAULT_MAX_ROWS,
            redirected: false,
            theme: None,
        }
    }
}

#[component]
pub fn TodoProgressPanel(props: &TodoProgressPanelProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let (visible, done, total) = build_todo_panel_rows(&props.items);

    // All finished (or empty) → render nothing (parent should also gate on this).
    if !todo_panel_should_show(&props.items) {
        return element! {
            View(width: 0u16, height: 0u16, flex_shrink: 0f32)
        };
    }

    let cap = if props.max_rows == 0 {
        visible.len()
    } else {
        props.max_rows.min(visible.len())
    };
    // When items exceed the cap, hide finished items from the tail one by one
    // while preserving original order. Stop when within cap or no more finished
    // items can be removed.
    let mut hidden_done_count = 0usize;
    let mut display: Vec<&TodoPanelRow> = visible.iter().collect();
    while display.len() > cap {
        match display.iter().rposition(|r| r.finished) {
            Some(idx) => {
                display.remove(idx);
                hidden_done_count += 1;
            }
            None => break,
        }
    }
    let hidden_count = visible.len().saturating_sub(display.len());
    let show_more = hidden_count > 0;
    let header = todo_panel_header_line(done, total, props.redirected);
    let border_color = if props.redirected {
        theme.border_subtle
    } else {
        theme.border
    };
    let title_color = if props.redirected {
        theme.text_muted
    } else {
        theme.text_hint
    };

    // Inner content width: border (2) + horizontal padding (2).
    let inner_width = props.width.saturating_sub(4).max(8);
    // Glyph + gap + label.
    let label_max = (inner_width as usize).saturating_sub(2).max(4);

    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    for row in display.iter() {
        let color = if row.finished {
            // Muted styling for completed items (iocraft has no strikethrough).
            theme.text_muted
        } else if props.redirected {
            theme.text_muted
        } else if row.running {
            theme.warning
        } else {
            theme.text_secondary
        };
        let glyph = todo_status_glyph(row.running, row.finished);
        let label = truncate_todo_label(&row.label, label_max);
        rows.push(
            element! {
                View(
                    flex_direction: FlexDirection::Row,
                    gap: 1u16,
                    align_items: AlignItems::Center,
                    flex_shrink: 0f32,
                    width: inner_width,
                ) {
                    View(flex_shrink: 0f32) {
                        Text(content: glyph.to_string(), color: color, wrap: TextWrap::NoWrap)
                    }
                    Text(
                        content: label,
                        color: color,
                        weight: if row.running && !props.redirected {
                            Weight::Bold
                        } else {
                            Weight::Normal
                        },
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            .into(),
        );
    }
    if show_more {
        let hidden_active = hidden_count.saturating_sub(hidden_done_count);
        let more_label = match (hidden_done_count, hidden_active) {
            (d, 0) if d > 0 => format!("... +{hidden_count} more ({d} done)"),
            (0, a) if a > 0 => format!("... +{hidden_count} more ({a} active)"),
            (d, a) => format!("... +{hidden_count} more ({a} active, {d} done)"),
        };
        rows.push(
            element! {
                Text(
                    content: more_label,
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
            border_style: BorderStyle::Round,
            border_color: border_color,
            position: Position::Relative,
            padding_left: 1u16,
            padding_right: 1u16,
            padding_top: 0u16,
            padding_bottom: 0u16,
            margin_bottom: 0u16,
        ) {
            View(
                position: Position::Absolute,
                top: 0,
                left: 1,
                margin_top: -1,
                background_color: Color::Reset,
            ) {
                Text(
                    content: format!(" {header} "),
                    color: title_color,
                    wrap: TextWrap::NoWrap,
                )
            }
            View(
                width: inner_width,
                flex_direction: FlexDirection::Column,
                gap: 0u16,
                flex_shrink: 0f32,
            ) {
                #(rows)
            }
        }
    }
}

/// Char-safe truncate with ellipsis for long todo titles.
fn truncate_todo_label(label: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = label.chars().count();
    if count <= max_chars {
        return label.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = label.chars().take(keep).collect();
    out.push('…');
    out
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
        assert_eq!(todo_status_glyph(true, false), "◐");
        assert_eq!(todo_status_glyph(false, true), "✓");
        assert_eq!(todo_status_word(false, false), "pending");
        assert_eq!(todo_status_word(false, true), "running");
        assert_eq!(todo_status_word(true, false), "done");
    }

    #[test]
    fn all_rows_stay_visible_including_finished() {
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
        let (visible, done, total) = build_todo_panel_rows(&rows);
        // All items stay in original order (including finished ones).
        assert_eq!(visible.len(), 4);
        assert_eq!(visible[0].label, "a");
        assert_eq!(visible[2].label, "c");
        assert_eq!(done, 2);
        assert_eq!(total, 4);
    }

    #[test]
    fn header_reports_counts_and_steered() {
        assert_eq!(todo_panel_header_line(1, 3, false), "Todos 1/3");
        assert_eq!(todo_panel_header_line(0, 3, false), "Todos 0/3");
        assert_eq!(todo_panel_header_line(2, 5, true), "Todos 2/5 · steered");
    }

    #[test]
    fn hide_when_all_items_finished() {
        // Empty list → hide.
        assert!(!todo_panel_should_show(&[]));
        // All done → hide (panel auto-hides once every task completes).
        let done = vec![TodoPanelRow {
            label: "done".into(),
            running: false,
            finished: true,
        }];
        assert!(!todo_panel_should_show(&done));
        // Open items → show.
        let open = vec![TodoPanelRow {
            label: "open".into(),
            running: false,
            finished: false,
        }];
        assert!(todo_panel_should_show(&open));
        // Mix of done + running → show.
        let mixed = vec![
            TodoPanelRow {
                label: "done".into(),
                running: false,
                finished: true,
            },
            TodoPanelRow {
                label: "running".into(),
                running: true,
                finished: false,
            },
        ];
        assert!(todo_panel_should_show(&mixed));
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
        let (visible, _, _) = build_todo_panel_rows(&rows);
        assert_eq!(visible.len(), 8);
        assert_eq!(visible.len().saturating_sub(TODO_PANEL_DEFAULT_MAX_ROWS), 3);
    }

    #[test]
    fn truncate_label_is_char_safe() {
        assert_eq!(truncate_todo_label("hello", 10), "hello");
        assert_eq!(truncate_todo_label("hello world", 8), "hello w…");
        assert_eq!(truncate_todo_label("日本語テスト", 4), "日本語…");
    }

    #[test]
    fn items_preserve_original_order() {
        // Items stay in their original order — no reordering.
        let rows = vec![
            TodoPanelRow {
                label: "done1".into(),
                running: false,
                finished: true,
            },
            TodoPanelRow {
                label: "pending1".into(),
                running: false,
                finished: false,
            },
            TodoPanelRow {
                label: "done2".into(),
                running: false,
                finished: true,
            },
            TodoPanelRow {
                label: "running1".into(),
                running: true,
                finished: false,
            },
            TodoPanelRow {
                label: "pending2".into(),
                running: false,
                finished: false,
            },
        ];
        let (visible, done, total) = build_todo_panel_rows(&rows);
        assert_eq!(total, 5);
        assert_eq!(done, 2);
        // Original order preserved.
        assert_eq!(
            visible.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            vec!["done1", "pending1", "done2", "running1", "pending2"]
        );
    }

    #[test]
    fn cap_hides_finished_items_from_tail() {
        // When items exceed the cap, finished items are removed from the tail.
        let rows = vec![
            TodoPanelRow {
                label: "a".into(),
                running: false,
                finished: false,
            },
            TodoPanelRow {
                label: "b".into(),
                running: false,
                finished: true,
            },
            TodoPanelRow {
                label: "c".into(),
                running: false,
                finished: false,
            },
            TodoPanelRow {
                label: "d".into(),
                running: false,
                finished: true,
            },
            TodoPanelRow {
                label: "e".into(),
                running: false,
                finished: false,
            },
        ];
        // Simulate the panel's cap logic: cap=4, 5 items → hide 1 finished from tail.
        let (visible, _done, _total) = build_todo_panel_rows(&rows);
        let cap = 4;
        let mut display: Vec<&TodoPanelRow> = visible.iter().collect();
        let mut hidden_done = 0usize;
        while display.len() > cap {
            if let Some(idx) = display.iter().rposition(|r| r.finished) {
                display.remove(idx);
                hidden_done += 1;
            } else {
                break;
            }
        }
        assert_eq!(display.len(), 4);
        assert_eq!(hidden_done, 1);
        // Last finished item ("d") removed; order preserved.
        assert_eq!(
            display.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c", "e"]
        );
    }
}
