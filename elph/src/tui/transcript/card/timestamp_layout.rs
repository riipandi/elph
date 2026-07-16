//! Right-aligned submit timestamps with reserved first-line width.

use chrono::{DateTime, Utc};
use elph_tui::utils::{display_width, truncate_with_ellipsis, wrap_text};
use iocraft::prelude::*;

use crate::tui::activity::{format_duration_secs, format_submitted_timestamp_suffix};
use crate::tui::theme::TOOL_ARGS_FG;

/// Build the dimmed right-rail label (`1.2s 14:32`).
pub fn user_input_right_rail(submitted_at: Option<DateTime<Utc>>, duration_secs: Option<f64>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(secs) = duration_secs {
        parts.push(format_duration_secs(secs));
    }
    if let Some(at) = submitted_at {
        parts.push(format_submitted_timestamp_suffix(at));
    }
    (!parts.is_empty()).then_some(parts.join(" "))
}

/// Wrapped body lines (`right_rail` occupies the right edge of row 0 only).
pub fn layout_user_input_lines(content: &str, right_rail: Option<&str>, wrap_width: u16) -> Vec<String> {
    let width = wrap_width.max(1) as usize;
    let Some(rail) = right_rail.filter(|s| !s.is_empty()) else {
        return wrap_user_input_block(content, width);
    };
    let first_budget = width.saturating_sub(display_width(rail)).max(1);
    match content.split_once('\n') {
        Some((first, rest)) => {
            let mut lines = wrap_text(first, first_budget);
            if !rest.is_empty() {
                lines.extend(wrap_user_input_block(rest, width));
            }
            lines
        }
        None => wrap_text(content, first_budget),
    }
}

pub fn layout_sticky_first_line(content: &str, right_rail: Option<&str>, inner_width: u16) -> String {
    let first = content.lines().next().unwrap_or("");
    let Some(rail) = right_rail.filter(|s| !s.is_empty()) else {
        return first.to_string();
    };
    let budget = inner_width.max(1).saturating_sub(display_width(rail) as u16).max(1) as usize;
    truncate_with_ellipsis(first, budget)
}

fn wrap_user_input_block(content: &str, width: usize) -> Vec<String> {
    if content.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    for part in content.split('\n') {
        let wrapped = wrap_text(part, width);
        if wrapped.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(wrapped);
        }
    }
    lines
}

/// Pre-wrapped lines with an optional right-rail label pinned on the first row.
pub fn render_user_input_lines(
    inner_width: u16,
    lines: &[String],
    right_rail: Option<&str>,
    foreground: Color,
    rail_color: Color,
) -> AnyElement<'static> {
    let width = inner_width.max(1);
    let mut row_elements: Vec<AnyElement<'static>> = Vec::new();
    if let Some(first) = lines.first() {
        row_elements.push(render_timestamp_header_row(width, first, right_rail, foreground, rail_color));
    } else if right_rail.is_some() {
        row_elements.push(render_timestamp_header_row(width, "", right_rail, foreground, rail_color));
    }
    for line in lines.iter().skip(1) {
        row_elements.push(
            element! {
                View(width: width, flex_shrink: 0f32) {
                    Text(color: foreground, wrap: TextWrap::NoWrap, content: line.clone())
                }
            }
            .into(),
        );
    }
    element! {
        View(
            width: width,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            gap: 0,
        ) {
            #(row_elements)
        }
    }
    .into()
}

fn render_timestamp_header_row(
    width: u16,
    content: &str,
    right_rail: Option<&str>,
    foreground: Color,
    rail_color: Color,
) -> AnyElement<'static> {
    let rail_chip: Option<AnyElement<'static>> = right_rail.filter(|s| !s.is_empty()).map(|label| {
        element! {
            Text(color: rail_color, wrap: TextWrap::NoWrap, content: label.to_string())
        }
        .into()
    });
    element! {
        View(
            width: width,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::FlexStart,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            View(flex_grow: 1f32, flex_shrink: 1f32, overflow: Overflow::Hidden) {
                Text(color: foreground, wrap: TextWrap::NoWrap, content: content.to_string())
            }
            #(rail_chip)
        }
    }
    .into()
}

pub fn render_sticky_prompt_row(
    inner_width: u16,
    display_content: &str,
    right_rail: Option<&str>,
    foreground: Color,
) -> AnyElement<'static> {
    let lines: Vec<String> = display_content.lines().map(str::to_string).collect();
    let first = layout_sticky_first_line(display_content, right_rail, inner_width);
    let width = inner_width.max(1);
    let mut row_elements: Vec<AnyElement<'static>> = vec![render_timestamp_header_row(
        width,
        &first,
        right_rail,
        foreground,
        TOOL_ARGS_FG,
    )];
    for line in lines.into_iter().skip(1) {
        row_elements.push(
            element! {
                View(width: width, flex_shrink: 0f32) {
                    Text(color: foreground, wrap: TextWrap::NoWrap, content: line)
                }
            }
            .into(),
        );
    }
    element! {
        View(
            width: width,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            gap: 0,
        ) {
            #(row_elements)
        }
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_reserves_rail_columns_on_first_line_only() {
        let rail = "14:32";
        let long = "word ".repeat(12).trim().to_string();
        let with_rail = layout_user_input_lines(&long, Some(rail), 20);
        let without_rail = layout_user_input_lines(&long, None, 20);
        assert!(with_rail.len() >= without_rail.len());
    }

    #[test]
    fn sticky_first_line_truncates_before_right_rail() {
        let rail = "14:32";
        let long = "x".repeat(40);
        let clipped = layout_sticky_first_line(&long, Some(rail), 24);
        assert!(clipped.ends_with('…'));
        let budget = 24usize.saturating_sub(display_width(rail));
        assert!(display_width(&clipped) <= budget);
    }
}
