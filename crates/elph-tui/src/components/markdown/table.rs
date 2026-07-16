//! GFM table layout and box-drawing grid rendering.

use iocraft::prelude::*;
use unicode_width::UnicodeWidthStr;

use crate::text_input_layout::WrappedTextLayout;
use crate::utils::truncate_with_ellipsis;

use super::model::MarkdownTable;
use super::theme::MarkdownTheme;

/// Absolute floor for any column (separator + at least one glyph).
const MIN_COL_WIDTH: u16 = 4;
/// Horizontal padding between cell text and the vertical grid rules on each side.
const CELL_PAD_X: u16 = 1;
/// Vertical bars in a content line: left edge + one between each column.
fn grid_vertical_bar_count(columns: usize) -> u16 {
    columns.saturating_add(1) as u16
}

/// Measured column widths and per-row terminal heights for one table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableLayout {
    pub col_widths: Vec<u16>,
    pub row_heights: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TableLine {
    Rule(String),
    Row { cell_segments: Vec<String>, header: bool },
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

    for index in 0..columns {
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
        plans[index].min = header_outer
            .max(word_outer)
            .min(plans[index].natural)
            .max(MIN_COL_WIDTH);
    }

    plans
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
            .map(
                |((&width, &min), &pin)| {
                    if pin { 0 } else { u32::from(width.saturating_sub(min)) }
                },
            )
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

/// When content-aware minimums exceed the budget, trim the widest columns first.
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

fn wrapped_cell_lines(text: &str, col_width: u16) -> Vec<String> {
    let text_width = cell_text_width(col_width);
    if text.is_empty() {
        return vec![String::new()];
    }
    WrappedTextLayout::new_for_overlay_editor(text, text_width).wrapped_line_strings(text)
}

fn pad_line_to_display_width(text: &str, width: u16) -> String {
    let width = width as usize;
    if text.width() <= width {
        let mut out = text.to_string();
        let deficit = width.saturating_sub(out.width());
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
    let deficit = (col_width as usize).saturating_sub(segment.width());
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

fn row_cell_segments(col_widths: &[u16], cell_lines: &[&str]) -> Vec<String> {
    col_widths
        .iter()
        .enumerate()
        .map(|(index, &width)| format_cell_segment(cell_lines.get(index).copied().unwrap_or(""), width))
        .collect()
}

#[cfg(test)]
fn table_line_display(line: &TableLine) -> String {
    match line {
        TableLine::Rule(text) => text.clone(),
        TableLine::Row { cell_segments, .. } => {
            let mut out = String::from("│");
            for (index, segment) in cell_segments.iter().enumerate() {
                out.push_str(segment);
                if index + 1 < cell_segments.len() {
                    out.push('│');
                }
            }
            out.push('│');
            out
        }
    }
}

/// Compute wrapped column widths and row heights for a markdown table matrix.
pub fn layout_markdown_table(table: &MarkdownTable, max_width: u16) -> Option<TableLayout> {
    if table.rows.is_empty() {
        return None;
    }
    let columns = column_count(table);
    if columns == 0 {
        return None;
    }

    let col_widths = compute_column_widths(table, max_width);
    let row_heights = table
        .rows
        .iter()
        .map(|row| {
            let cells = normalize_row(row, columns);
            cells
                .iter()
                .enumerate()
                .map(|(index, cell)| cell_wrap_rows(cell, col_widths[index]))
                .max()
                .unwrap_or(1)
        })
        .collect();

    Some(TableLayout {
        col_widths,
        row_heights,
    })
}

fn cell_wrap_rows(text: &str, col_width: u16) -> u16 {
    wrapped_cell_lines(text, col_width).len().max(1) as u16
}

fn build_table_lines(table: &MarkdownTable, max_width: u16) -> Option<Vec<TableLine>> {
    let layout = layout_markdown_table(table, max_width)?;
    let columns = layout.col_widths.len();
    if columns == 0 {
        return None;
    }

    let mut lines = Vec::new();
    lines.push(TableLine::Rule(horizontal_rule(&layout.col_widths, '┌', '┬', '┐')));

    for (row_index, row) in table.rows.iter().enumerate() {
        let cells = normalize_row(row, columns);
        let wrapped: Vec<Vec<String>> = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| wrapped_cell_lines(cell, layout.col_widths[index]))
            .collect();
        let logical_height = wrapped.iter().map(|lines| lines.len()).max().unwrap_or(1);
        let header = row_index == 0;

        for line_index in 0..logical_height {
            let cell_refs: Vec<&str> = wrapped
                .iter()
                .map(|cell| cell.get(line_index).map(String::as_str).unwrap_or(""))
                .collect();
            lines.push(TableLine::Row {
                cell_segments: row_cell_segments(&layout.col_widths, &cell_refs),
                header,
            });
        }

        if row_index + 1 < table.rows.len() {
            lines.push(TableLine::Rule(horizontal_rule(&layout.col_widths, '├', '┼', '┤')));
        }
    }

    lines.push(TableLine::Rule(horizontal_rule(&layout.col_widths, '└', '┴', '┘')));
    Some(lines)
}

/// Terminal row budget for scroll layout.
pub fn markdown_table_row_count(table: &MarkdownTable, max_width: u16) -> u16 {
    build_table_lines(table, max_width)
        .map(|lines| lines.len() as u16)
        .unwrap_or(0)
        .max(1)
}

fn render_table_line(line: TableLine, width: u16, theme: &MarkdownTheme) -> AnyElement<'static> {
    match line {
        TableLine::Rule(text) => element! {
            View(width: width.max(1), flex_shrink: 0f32) {
                Text(
                    content: text,
                    color: theme.table_border,
                    wrap: TextWrap::NoWrap,
                )
            }
        }
        .into(),
        TableLine::Row { cell_segments, header } => {
            let content_color = if header { theme.table_header } else { theme.body };
            let content_weight = if header { Weight::Bold } else { Weight::Normal };
            let mut parts = vec![MixedTextContent::new("│").color(theme.table_border)];
            for (index, segment) in cell_segments.iter().enumerate() {
                let mut part = MixedTextContent::new(segment.as_str()).color(content_color);
                if header {
                    part = part.weight(content_weight);
                }
                parts.push(part);
                if index + 1 < cell_segments.len() {
                    parts.push(MixedTextContent::new("│").color(theme.table_border));
                }
            }
            parts.push(MixedTextContent::new("│").color(theme.table_border));
            element! {
                View(width: width.max(1), flex_shrink: 0f32) {
                    MixedText(contents: parts, wrap: TextWrap::NoWrap)
                }
            }
            .into()
        }
    }
}

/// Render a markdown table as a box-drawing grid with padded cells.
pub fn render_markdown_table(
    table: &MarkdownTable,
    width: u16,
    theme: &MarkdownTheme,
    margin_bottom: u16,
) -> Option<AnyElement<'static>> {
    let lines = build_table_lines(table, width)?;
    let row_elements: Vec<AnyElement<'static>> = lines
        .into_iter()
        .map(|line| render_table_line(line, width, theme))
        .collect();

    Some(
        element! {
            View(
                width: width.max(1),
                margin_bottom: margin_bottom,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(row_elements)
            }
        }
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::markdown::parse_markdown_document;

    #[test]
    fn layout_respects_emoji_display_width() {
        let table = MarkdownTable {
            rows: vec![vec!["Name".into(), "Status".into()], vec!["Ada".into(), "✅".into()]],
        };
        let layout = layout_markdown_table(&table, 40).expect("layout");
        assert_eq!(layout.col_widths.len(), 2);
        assert!(layout.col_widths[0] >= cell_outer_width("Name"));
        assert!(layout.col_widths[1] >= cell_outer_width("Status"));
    }

    #[test]
    fn narrow_table_keeps_header_columns_readable() {
        let table = MarkdownTable {
            rows: vec![
                vec!["Feature".into(), "Status".into(), "Owner".into()],
                vec!["Longer feature description".into(), "Done".into(), "Ada".into()],
            ],
        };
        let layout = layout_markdown_table(&table, 28).expect("layout");
        assert!(layout.col_widths[1] >= cell_outer_width("Status"));
        assert!(layout.col_widths[2] >= cell_outer_width("Owner"));
        let total: u16 = layout.col_widths.iter().sum::<u16>() + grid_vertical_bar_count(3);
        assert!(total <= 28);
    }

    #[test]
    fn surplus_width_favors_heavier_content_columns() {
        let table = MarkdownTable {
            rows: vec![
                vec!["ID".into(), "Notes".into()],
                vec!["1".into(), "short".into()],
                vec!["2".into(), "much longer body copy".into()],
            ],
        };
        let layout = layout_markdown_table(&table, 60).expect("layout");
        assert!(layout.col_widths[1] > layout.col_widths[0]);
    }

    #[test]
    fn narrow_width_wraps_cells_and_increases_row_count() {
        let table = MarkdownTable {
            rows: vec![vec!["Header".into()], vec!["abcdefghijklmnopqrstuvwxyz".into()]],
        };
        let rows = markdown_table_row_count(&table, 12);
        assert!(rows > 3, "wrapped cell should add terminal rows, got {rows}");
    }

    #[test]
    fn row_count_matches_rendered_lines() {
        let table = MarkdownTable {
            rows: vec![vec!["A".into(), "B".into()], vec!["one".into(), "two".into()]],
        };
        let counted = markdown_table_row_count(&table, 30);
        let rendered = build_table_lines(&table, 30).expect("lines").len() as u16;
        assert_eq!(counted, rendered);
    }

    #[test]
    fn layout_reserves_width_for_column_separators() {
        let two_cols = layout_markdown_table(
            &MarkdownTable {
                rows: vec![vec!["A".into(), "B".into()]],
            },
            30,
        )
        .expect("two cols");
        let three_cols = layout_markdown_table(
            &MarkdownTable {
                rows: vec![vec!["A".into(), "B".into(), "C".into()]],
            },
            30,
        )
        .expect("three cols");
        let two_sum: u16 = two_cols.col_widths.iter().sum();
        let three_sum: u16 = three_cols.col_widths.iter().sum();
        assert!(
            three_sum <= two_sum,
            "extra column separators should not expand past inner width"
        );
    }

    #[test]
    fn grid_uses_cross_junctions_and_cell_padding() {
        let doc = parse_markdown_document("| Name | Status |\n| --- | --- |\n| Ada | ✅ |");
        let table = doc.lines.iter().find_map(|line| line.table.as_ref()).expect("table");
        let lines = build_table_lines(table, 40).expect("lines");
        let body = lines
            .iter()
            .find(|line| table_line_display(line).contains("Ada"))
            .expect("body line");
        assert!(
            lines
                .iter()
                .any(|line| matches!(line, TableLine::Rule(text) if text.contains('┼'))),
            "expected cross junctions"
        );
        assert!(
            table_line_display(body).contains(&format!(" {} ", "Ada")),
            "expected padded cell content, got: {}",
            table_line_display(body)
        );
    }

    #[test]
    fn parsed_gfm_table_renders_grid() {
        let doc = parse_markdown_document("| Name | Status |\n| --- | --- |\n| Ada | ✅ |");
        let table = doc.lines.iter().find_map(|line| line.table.as_ref()).expect("table");
        let block = render_markdown_table(table, 40, &MarkdownTheme::default(), 0).expect("render");
        let rendered = element! { View(width: 40) { #(vec![block]) } }.to_string();
        assert!(rendered.contains('┼'));
        assert!(rendered.contains("Ada"));
        assert!(rendered.contains('✅'));
    }
}
