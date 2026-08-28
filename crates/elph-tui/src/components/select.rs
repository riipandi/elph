//! Vertical list selector (OpenTUI Select analogue).

use crate::types::SelectOption;
use crate::wrapped_text_row_count;
use iocraft::prelude::*;

use super::theme::{
    LIST_MARKER_COL, UiTheme, dialog_marker_color, dialog_option_desc_style, dialog_option_name_style,
    dialog_row_surface, list_marker, list_row_desc_style, list_row_name_style, resolve_ui_theme,
};

/// `height: 0` lets the list grow to fit every option (dialog-friendly).
pub const SELECT_LIST_AUTO_HEIGHT: u16 = 0;

/// Props for [`SelectList`].
#[derive(Default, Props)]
pub struct SelectListProps {
    pub width: u16,
    pub height: u16,
    pub options: Vec<SelectOption>,
    pub selected_index: Option<State<usize>>,
    pub has_focus: bool,
    pub show_description: bool,
    pub fast_scroll_step: usize,
    pub theme: Option<UiTheme>,
    /// Tighter row inset and inter-row gap for inline shell dialogs.
    pub compact: bool,
    /// Render description on the same line as the name (instead of below it).
    pub inline_description: bool,
    pub on_change: HandlerMut<'static, usize>,
    /// Suppress "↑ N more / ↓ N more" overflow indicators (like ModelOptionList).
    pub hide_more_overflow: bool,
}

fn select_row_surface(theme: UiTheme, selected: bool, compact: bool) -> Color {
    if !selected {
        Color::Reset
    } else if compact {
        dialog_row_surface(theme, true)
    } else {
        theme.selection_bg
    }
}

/// Display rows one option consumes (single-line description estimate).
pub fn select_option_rows(show_description: bool, description: &str) -> usize {
    if show_description && !description.is_empty() {
        2
    } else {
        1
    }
}

/// Text column width beside the selection marker.
pub fn select_content_width(theme: UiTheme, list_width: u16, compact: bool) -> u16 {
    let inner = if compact {
        list_width
    } else {
        select_inner_width(theme, list_width)
    };
    inner
        .saturating_sub(LIST_MARKER_COL)
        .saturating_sub(theme.gap_md)
        .max(1)
}

/// Vertical gap between stacked options.
pub fn select_list_item_gap(_compact: bool, theme: UiTheme) -> u16 {
    theme.gap_sm
}

/// Row counts for all options.
pub fn select_row_counts(options: &[SelectOption], show_description: bool) -> Vec<usize> {
    options
        .iter()
        .map(|opt| select_option_rows(show_description, &opt.description))
        .collect()
}

/// Wrap-aware row counts for layout and viewport sizing.
pub fn select_measured_row_counts(
    options: &[SelectOption],
    show_description: bool,
    list_width: u16,
    theme: UiTheme,
    compact: bool,
) -> Vec<usize> {
    let content_width = select_content_width(theme, list_width, compact);
    options
        .iter()
        .map(|opt| {
            let mut rows = 1usize;
            if show_description && !opt.description.is_empty() {
                rows += wrapped_text_row_count(&opt.description, content_width as usize);
            }
            rows
        })
        .collect()
}

/// Total display rows for a list, including inter-item spacing inside the viewport.
pub fn select_list_total_rows(
    options: &[SelectOption],
    show_description: bool,
    list_width: u16,
    theme: UiTheme,
    compact: bool,
) -> usize {
    let row_counts = select_measured_row_counts(options, show_description, list_width, theme, compact);
    let items: usize = row_counts.iter().sum();
    if options.is_empty() {
        return 1;
    }
    let gaps = select_list_item_gap(compact, theme) as usize * options.len().saturating_sub(1);
    items.saturating_add(gaps).max(1)
}

/// Resolve viewport rows (`0` height prop = fit all options).
pub fn select_resolve_viewport_rows(
    options: &[SelectOption],
    show_description: bool,
    list_width: u16,
    theme: UiTheme,
    height: u16,
    compact: bool,
) -> (usize, usize) {
    let total = select_list_total_rows(options, show_description, list_width, theme, compact);
    let cap = if height == SELECT_LIST_AUTO_HEIGHT {
        total
    } else {
        height.max(1) as usize
    };
    (total.max(1).min(cap), total)
}

/// Label text for one [`SelectList`] row (single-line helper).
pub fn select_option_line(prefix: &str, name: &str, description: &str, show_description: bool) -> String {
    let desc = if show_description && !description.is_empty() {
        format!("\n  {description}")
    } else {
        String::new()
    };
    format!("{prefix}{name}{desc}")
}

/// Row prefix for a [`SelectList`] option.
pub fn select_row_prefix(selected: bool) -> &'static str {
    list_marker(selected)
}

/// Row colors for a [`SelectList`] option name.
pub fn select_row_colors(theme: UiTheme, selected: bool) -> (Color, Weight) {
    list_row_name_style(theme, selected)
}

/// Clamped selected index for a non-empty option list.
pub fn select_clamped_index(selected: usize, len: usize) -> usize {
    if len == 0 { 0 } else { selected.min(len - 1) }
}

/// Next selection index after a key press.
pub fn select_key_to_index(current: usize, len: usize, code: KeyCode, modifiers: KeyModifiers, step: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let fast = modifiers.contains(KeyModifiers::SHIFT);
    let Some(delta) = select_key_delta(code, fast, step) else {
        return current;
    };
    if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        (current + delta as usize).min(len - 1)
    }
}

/// Selection change from a key press (`None` = no change).
pub fn select_key_delta(code: KeyCode, fast: bool, step: usize) -> Option<isize> {
    let delta = if fast { step as isize } else { 1 };
    match code {
        KeyCode::Up => Some(-delta),
        KeyCode::Down => Some(delta),
        _ => None,
    }
}

/// First visible option for a boundary-aware selection window.
pub fn select_window_start(selected: usize, height: usize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let viewport = height.max(1).min(len);
    selected
        .saturating_sub(viewport.saturating_sub(1))
        .min(len.saturating_sub(viewport))
}

/// First option index to show when viewport is measured in display rows.
pub fn select_window_start_for_rows(selected: usize, viewport_rows: usize, row_counts: &[usize]) -> usize {
    let len = row_counts.len();
    if len == 0 || viewport_rows == 0 {
        return 0;
    }
    let total: usize = row_counts.iter().sum();
    if total <= viewport_rows {
        return 0;
    }
    let selected = selected.min(len - 1);
    let mut start = selected;
    let mut used_rows = row_counts[selected].max(1);
    while start > 0 {
        let preceding_rows = row_counts[start - 1].max(1);
        if used_rows.saturating_add(preceding_rows) > viewport_rows {
            break;
        }
        start -= 1;
        used_rows += preceding_rows;
    }
    start
}

/// How many display rows are hidden above the current window.
pub fn select_hidden_rows_above(window_start: usize, row_counts: &[usize]) -> usize {
    row_counts.iter().take(window_start).sum()
}

/// How many display rows are hidden below the visible slice.
pub fn select_hidden_rows_below(window_start: usize, visible_rows: usize, row_counts: &[usize]) -> usize {
    let total: usize = row_counts.iter().sum();
    let above = select_hidden_rows_above(window_start, row_counts);
    total.saturating_sub(above + visible_rows)
}

/// Inner content width inside list padding and border chrome.
pub fn select_inner_width(theme: UiTheme, width: u16) -> u16 {
    theme.list_viewport_inner_width(width)
}

/// Keyboard-navigable option list.
#[component]
pub fn SelectList(props: &mut SelectListProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let internal_index = hooks.use_state(|| 0usize);
    let mut selected = props.selected_index.unwrap_or(internal_index);
    let step = props.fast_scroll_step.max(1);
    let has_focus = props.has_focus;
    let options = props.options.clone();
    let show_description = props.show_description;
    let theme = resolve_ui_theme(&hooks, props.theme);
    let compact = props.compact;
    let row_inset = if compact { 0 } else { theme.container_inset() };
    let inner_width = if compact {
        props.width
    } else {
        select_inner_width(theme, props.width)
    };
    let item_gap = select_list_item_gap(compact, theme);
    let content_width = select_content_width(theme, props.width, compact);
    let (viewport_rows, total_rows) =
        select_resolve_viewport_rows(&options, show_description, props.width, theme, props.height, compact);
    let scrollable = total_rows > viewport_rows;

    let option_count = options.len();
    hooks.use_terminal_events({
        let mut on_change = props.on_change.take();
        move |event| {
            if !has_focus {
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

            let len = option_count;
            if len == 0 {
                return;
            }

            let prev = selected.get();
            let next = select_key_to_index(prev, len, code, modifiers, step);
            if next != prev {
                selected.set(next);
                if !on_change.is_default() {
                    on_change(next);
                }
            }
        }
    });

    let len = options.len();
    let index = select_clamped_index(selected.get(), len);
    let row_counts = select_measured_row_counts(&options, show_description, props.width, theme, compact);
    let window_start = select_window_start_for_rows(index, viewport_rows, &row_counts);
    // The overflow marker consumes one row. Recompute the item window after
    // scrolling starts so the selected option can still reach the bottom edge.
    let window_start = if !props.hide_more_overflow && select_hidden_rows_above(window_start, &row_counts) > 0 {
        select_window_start_for_rows(index, viewport_rows.saturating_sub(1), &row_counts)
    } else {
        window_start
    };

    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    let mut used_rows = 0usize;

    if len == 0 {
        rows.push(
            element! {
                Text(content: "(no options)".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
            }
            .into(),
        );
    } else {
        let hidden_above = select_hidden_rows_above(window_start, &row_counts);
        if !props.hide_more_overflow && hidden_above > 0 {
            rows.push(
                element! {
                    Text(
                        content: format!("  ↑ {hidden_above} more"),
                        color: theme.text_hint,
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            );
            used_rows += 1;
        }

        for (i, opt) in options.iter().enumerate().skip(window_start) {
            let item_rows = row_counts[i];
            if used_rows > 0 && used_rows + item_rows > viewport_rows {
                break;
            }

            let selected_row = i == index;
            let marker = list_marker(selected_row);
            let (name_color, name_weight) = if compact {
                dialog_option_name_style(theme, selected_row)
            } else {
                list_row_name_style(theme, selected_row)
            };
            let desc_color = if compact {
                dialog_option_desc_style(theme, selected_row)
            } else {
                list_row_desc_style(theme, selected_row)
            };
            let desc_color = opt.description_color.unwrap_or(desc_color);
            let show_desc = show_description && !opt.description.is_empty();

            let inner_content: AnyElement<'static> = if props.inline_description && show_desc {
                // Inline: name and hint on the same row
                let desc_w = (content_width / 3).clamp(12, 20);
                let name_w = content_width.saturating_sub(desc_w).saturating_sub(1).max(4);
                element! {
                    View(
                        width: content_width,
                        flex_direction: FlexDirection::Row,
                        gap: 1,
                        flex_shrink: 0f32,
                        align_items: AlignItems::Center,
                    ) {
                        View(width: name_w, flex_shrink: 0f32) {
                            Text(
                                content: opt.name.clone(),
                                color: name_color,
                                weight: name_weight,
                                wrap: TextWrap::NoWrap,
                            )
                        }
                        View(width: desc_w, flex_shrink: 0f32) {
                            Text(
                                content: opt.description.clone(),
                                color: desc_color,
                                wrap: TextWrap::NoWrap,
                            )
                        }
                    }
                }
                .into()
            } else {
                // Classic: name above, description below
                element! {
                    View(
                        width: content_width,
                        flex_direction: FlexDirection::Column,
                        gap: 0,
                        flex_shrink: 0f32,
                    ) {
                        Text(
                            content: opt.name.clone(),
                            color: name_color,
                            weight: name_weight,
                            wrap: TextWrap::NoWrap,
                        )
                        #(if show_desc {
                            Some(element! {
                                Text(
                                    content: opt.description.clone(),
                                    color: desc_color,
                                    wrap: TextWrap::Wrap,
                                )
                            })
                        } else {
                            None
                        })
                    }
                }
                .into()
            };

            rows.push(
                element! {
                    View(
                        width: inner_width,
                        flex_direction: FlexDirection::Row,
                        gap: theme.gap_md,
                        align_items: AlignItems::FlexStart,
                        background_color: select_row_surface(theme, selected_row, compact),
                        padding_left: row_inset,
                        padding_right: row_inset,
                    ) {
                        View(width: LIST_MARKER_COL, flex_shrink: 0f32) {
                            Text(
                                content: marker.to_string(),
                                color: if compact {
                                    dialog_marker_color(theme, selected_row)
                                } else {
                                    theme.list_marker_color(selected_row)
                                },
                                wrap: TextWrap::NoWrap,
                            )
                        }
                        #(inner_content)
                    }
                }
                .into(),
            );
            used_rows += item_rows;
        }

        let rows_before = select_hidden_rows_above(window_start, &row_counts);
        let rows_shown = if props.hide_more_overflow {
            used_rows
        } else {
            used_rows.saturating_sub(usize::from(hidden_above > 0))
        };
        let hidden_below = row_counts
            .iter()
            .sum::<usize>()
            .saturating_sub(rows_before + rows_shown);
        if !props.hide_more_overflow && hidden_below > 0 {
            rows.push(
                element! {
                    Text(
                        content: format!("  ↓ {hidden_below} more"),
                        color: theme.text_hint,
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            );
        }
    }

    if scrollable {
        element! {
            View(
                width: props.width,
                height: viewport_rows as u16,
                min_height: viewport_rows as u16,
                flex_direction: FlexDirection::Column,
                gap: item_gap,
                background_color: Color::Reset,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
                #(rows)
            }
        }
    } else {
        element! {
            View(
                width: props.width,
                flex_direction: FlexDirection::Column,
                gap: item_gap,
                background_color: Color::Reset,
                flex_shrink: 0f32,
            ) {
                #(rows)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_counts_reflect_descriptions() {
        let options = vec![SelectOption::new("a", ""), SelectOption::new("b", "detail")];
        assert_eq!(select_row_counts(&options, false), vec![1, 1]);
        assert_eq!(select_row_counts(&options, true), vec![1, 2]);
    }

    #[test]
    fn window_start_for_rows_moves_only_at_viewport_edges() {
        let counts = vec![1; 20];
        assert_eq!(select_window_start_for_rows(4, 5, &counts), 0);
        assert_eq!(select_window_start_for_rows(5, 5, &counts), 1);
        assert_eq!(select_window_start_for_rows(19, 5, &counts), 15);
    }

    #[test]
    fn window_start_for_rows_keeps_variable_height_selection_visible() {
        let counts = vec![1, 1, 2, 1, 2, 1];
        assert_eq!(select_window_start_for_rows(3, 4, &counts), 1);
        assert_eq!(select_window_start_for_rows(4, 4, &counts), 3);
    }

    #[test]
    fn auto_height_fits_all_mode_options() {
        let theme = UiTheme::default();
        let options = crate::types::DialogAgentMode::all()
            .into_iter()
            .map(|mode| SelectOption::new(mode.label(), mode.description()))
            .collect::<Vec<_>>();
        let total = select_list_total_rows(&options, true, 48, theme, false);
        let (viewport, _) = select_resolve_viewport_rows(&options, true, 48, theme, SELECT_LIST_AUTO_HEIGHT, false);
        assert_eq!(viewport, total);
        assert!(total >= 8);
    }
}
