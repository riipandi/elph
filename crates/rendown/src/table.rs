//! GFM table layout and box-drawing grid (text output for ANSI).

use unicode_width::UnicodeWidthStr;

use crate::model::{FontWeight, MarkdownTable, StyledSpan};
use crate::theme::MarkdownTheme;
use crate::wrap::{display_width, truncate_with_ellipsis, wrap_text_to_lines};

const MIN_COL_WIDTH: u16 = 4;
const CELL_PAD_X: u16 = 1;

fn grid_vertical_bar_count(columns: usize) -> u16 {
    columns.saturating_add(1) as u16
}

fn column_count(table: &MarkdownTable) -> usize {
    table.rows.iter().map(|row| row.len()).max().unwrap_or(0)
}

fn cell_display_width(text: &str) -> u16 {
    if text.is_empty() { 0 } else { text.width() as u16 }
}

fn longest_word_display_width(text: &str) -> u16 {
    text.split_whitespace().map(UnicodeWidthStr::width).max().unwrap_or(0) as u16
}

fn cell_outer_width(text: &str) -> u16 {
    let content = cell_display_width(text);
    if content == 0 {
        MIN_COL_WIDTH
    } else {
        content.saturating_add(CELL_PAD_X.saturating_mul(2)).max(MIN_COL_WIDTH)
    }
}

fn normalize_row(row: &[String], columns: usize) -> Vec<String> {
    let mut cells = row.to_vec();
    cells.resize(columns, String::new());
    cells
}

fn cell_text_width(col_width: u16) -> u16 {
    col_width.saturating_sub(CELL_PAD_X.saturating_mul(2)).max(1)
}

fn grid_content_budget(max_width: u16, columns: usize) -> u16 {
    max_width
        .max(grid_vertical_bar_count(columns).saturating_add(MIN_COL_WIDTH))
        .saturating_sub(grid_vertical_bar_count(columns))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColumnWidthPlan {
    natural: u16,
    min: u16,
    weight: u32,
}

fn measure_column_plans(table: &MarkdownTable, columns: usize) -> Vec<ColumnWidthPlan> {
    let mut plans = vec![
        ColumnWidthPlan {
            natural: MIN_COL_WIDTH,
            min: MIN_COL_WIDTH,
            weight: 1,
        };
        columns
    ];

    for row in &table.rows {
        let cells = normalize_row(row, columns);
        for (index, cell) in cells.iter().enumerate() {
            let outer = cell_outer_width(cell);
            plans[index].natural = plans[index].natural.max(outer);
            plans[index].weight = plans[index]
                .weight
                .saturating_add(u32::from(cell_display_width(cell).max(1)));
        }
    }

    for (index, plan) in plans.iter_mut().enumerate().take(columns) {
        let header = table
            .rows
            .first()
            .map(|row| normalize_row(row, columns))
            .and_then(|cells| cells.get(index).cloned())
            .unwrap_or_default();
        let longest_word = table
            .rows
            .iter()
            .map(|row| longest_word_display_width(&normalize_row(row, columns)[index]))
            .max()
            .unwrap_or(0);
        let header_outer = cell_outer_width(&header);
        let word_outer = longest_word
            .saturating_add(CELL_PAD_X.saturating_mul(2))
            .max(MIN_COL_WIDTH);
        plan.min = header_outer.max(word_outer).min(plan.natural).max(MIN_COL_WIDTH);
    }

    plans
}

fn distribute_extra_width(widths: &mut [u16], extra: u16, weights: &[u32]) {
    if extra == 0 || widths.is_empty() {
        return;
    }
    let total_weight: u64 = weights.iter().map(|&w| u64::from(w)).sum();
    if total_weight == 0 {
        distribute_extra_width_evenly(widths, extra);
        return;
    }
    let mut remaining = extra;
    let last = widths.len().saturating_sub(1);
    for (index, width) in widths.iter_mut().enumerate() {
        if index == last {
            *width = width.saturating_add(remaining);
            break;
        }
        let share = (u64::from(extra) * u64::from(weights[index]) / total_weight) as u16;
        *width = width.saturating_add(share);
        remaining = remaining.saturating_sub(share);
    }
}

fn distribute_extra_width_evenly(widths: &mut [u16], extra: u16) {
    if extra == 0 || widths.is_empty() {
        return;
    }
    let column_count = widths.len() as u16;
    let mut remaining = extra;
    let last = widths.len().saturating_sub(1);
    for (index, width) in widths.iter_mut().enumerate() {
        if index == last {
            *width = width.saturating_add(remaining);
            break;
        }
        let share = extra / column_count;
        *width = width.saturating_add(share);
        remaining = remaining.saturating_sub(share);
    }
}

fn fit_column_widths(natural: &[u16], mins: &[u16], weights: &[u32], target: u16) -> Vec<u16> {
    let columns = natural.len();
    if columns == 0 {
        return Vec::new();
    }

    let natural_sum: u16 = natural.iter().sum();
    if natural_sum <= target {
        let mut widths = natural.to_vec();
        distribute_extra_width(&mut widths, target.saturating_sub(natural_sum), weights);
        return widths;
    }

    let min_sum: u16 = mins.iter().sum();
    if min_sum > target {
        return shrink_from_widest_columns(mins, natural, target);
    }

    let mut widths = natural.to_vec();
    let mut pinned = vec![false; columns];
    for _ in 0..=natural_sum {
        let total: u16 = widths.iter().sum();
        if total <= target {
            return widths;
        }
        let surplus = total.saturating_sub(target);
        let flex: Vec<u32> = widths
            .iter()
            .zip(mins.iter())
            .zip(pinned.iter())
            .map(|((&width, &min), &pin)| if pin { 0 } else { u32::from(width.saturating_sub(min)) })
            .collect();
        let flex_sum: u64 = flex.iter().map(|&value| u64::from(value)).sum();
        if flex_sum == 0 {
            let mut fallback = mins.to_vec();
            distribute_extra_width(&mut fallback, target.saturating_sub(min_sum), weights);
            return fallback;
        }

        let mut removed = 0u16;
        let last_free = (0..columns).rfind(|&index| !pinned[index]);
        for index in 0..columns {
            if pinned[index] {
                continue;
            }
            let share = if Some(index) == last_free {
                surplus.saturating_sub(removed)
            } else {
                (u64::from(surplus) * u64::from(flex[index]) / flex_sum) as u16
            };
            widths[index] = widths[index].saturating_sub(share);
            removed = removed.saturating_add(share);
            if widths[index] <= mins[index] {
                widths[index] = mins[index];
                pinned[index] = true;
            }
        }
    }
    widths
}

fn shrink_from_widest_columns(mins: &[u16], natural: &[u16], target: u16) -> Vec<u16> {
    let columns = mins.len();
    if columns == 0 {
        return Vec::new();
    }
    let min_total = MIN_COL_WIDTH.saturating_mul(columns as u16);
    if target <= min_total {
        return vec![MIN_COL_WIDTH; columns];
    }

    let mut widths = mins.to_vec();
    while widths.iter().sum::<u16>() > target {
        let candidate = widths
            .iter()
            .zip(natural.iter())
            .enumerate()
            .filter(|(_, pair)| pair.0 > &MIN_COL_WIDTH)
            .max_by(|left, right| left.1.1.cmp(right.1.1).then_with(|| left.1.0.cmp(right.1.0)))
            .map(|(index, _)| index);
        let Some(index) = candidate else {
            break;
        };
        widths[index] -= 1;
    }
    widths
}

fn compute_column_widths(table: &MarkdownTable, max_width: u16) -> Vec<u16> {
    let columns = column_count(table);
    if columns == 0 {
        return Vec::new();
    }
    let inner_width = grid_content_budget(max_width, columns);
    let plans = measure_column_plans(table, columns);
    let natural: Vec<u16> = plans.iter().map(|plan| plan.natural).collect();
    let mins: Vec<u16> = plans.iter().map(|plan| plan.min).collect();
    let weights: Vec<u32> = plans.iter().map(|plan| plan.weight).collect();
    fit_column_widths(&natural, &mins, &weights, inner_width)
}

fn wrapped_cell_lines(text: &str, col_width: u16) -> Vec<String> {
    let text_width = cell_text_width(col_width) as usize;
    if text.is_empty() {
        return vec![String::new()];
    }
    wrap_text_to_lines(text, text_width)
}

fn pad_line_to_display_width(text: &str, width: u16) -> String {
    let width = width as usize;
    if display_width(text) <= width {
        let mut out = text.to_string();
        let deficit = width.saturating_sub(display_width(&out));
        out.extend(std::iter::repeat_n(' ', deficit));
        return out;
    }
    truncate_with_ellipsis(text, width)
}

fn format_cell_segment(line: &str, col_width: u16) -> String {
    let text_width = cell_text_width(col_width);
    let mut segment = String::with_capacity(col_width as usize);
    segment.extend(std::iter::repeat_n(' ', CELL_PAD_X as usize));
    segment.push_str(&pad_line_to_display_width(line, text_width));
    let deficit = (col_width as usize).saturating_sub(display_width(&segment));
    segment.extend(std::iter::repeat_n(' ', deficit));
    segment
}

fn horizontal_rule(col_widths: &[u16], left: char, join: char, right: char) -> String {
    let mut line = String::new();
    line.push(left);
    for (index, &width) in col_widths.iter().enumerate() {
        line.extend(std::iter::repeat_n('─', width as usize));
        if index + 1 < col_widths.len() {
            line.push(join);
        }
    }
    line.push(right);
    line
}

/// One visual table line as styled spans ready for ANSI.
#[derive(Debug, Clone)]
pub struct StyledTableLine {
    pub spans: Vec<StyledSpan>,
}

/// Build box-drawing table lines as styled spans.
pub fn format_table_lines(table: &MarkdownTable, max_width: u16, theme: &MarkdownTheme) -> Vec<StyledTableLine> {
    if table.rows.is_empty() {
        return Vec::new();
    }
    let columns = column_count(table);
    if columns == 0 {
        return Vec::new();
    }
    let col_widths = compute_column_widths(table, max_width);
    let mut out = Vec::new();

    let push_rule = |text: String, out: &mut Vec<StyledTableLine>| {
        out.push(StyledTableLine {
            spans: vec![StyledSpan::plain(text, theme.table_border)],
        });
    };

    push_rule(horizontal_rule(&col_widths, '┌', '┬', '┐'), &mut out);

    for (row_index, row) in table.rows.iter().enumerate() {
        let cells = normalize_row(row, columns);
        let wrapped: Vec<Vec<String>> = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| wrapped_cell_lines(cell, col_widths[index]))
            .collect();
        let logical_height = wrapped.iter().map(|lines| lines.len()).max().unwrap_or(1);
        let header = row_index == 0;
        let content_color = if header { theme.table_header } else { theme.body };
        let content_weight = if header { FontWeight::Bold } else { FontWeight::Normal };

        for line_index in 0..logical_height {
            let mut spans = vec![StyledSpan::plain("│", theme.table_border)];
            for (index, cell_lines) in wrapped.iter().enumerate() {
                let segment = format_cell_segment(
                    cell_lines.get(line_index).map(String::as_str).unwrap_or(""),
                    col_widths[index],
                );
                spans.push(StyledSpan {
                    text: segment,
                    color: content_color,
                    weight: content_weight,
                    italic: false,
                    underline: false,
                    href: None,
                });
                spans.push(StyledSpan::plain("│", theme.table_border));
            }
            out.push(StyledTableLine { spans });
        }

        if row_index + 1 < table.rows.len() {
            push_rule(horizontal_rule(&col_widths, '├', '┼', '┤'), &mut out);
        }
    }

    push_rule(horizontal_rule(&col_widths, '└', '┴', '┘'), &mut out);
    out
}
