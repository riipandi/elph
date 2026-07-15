//! Text diff viewer (similar → colored lines).

use super::scroll_box::ScrollBox;
use iocraft::prelude::*;
use similar::{ChangeTag, TextDiff};

/// Diff display mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffMode {
    #[default]
    Unified,
    SideBySide,
}

/// Props for [`DiffView`].
#[derive(Clone, Default, Props)]
pub struct DiffViewProps {
    pub width: u16,
    pub height: u16,
    pub old_text: String,
    pub new_text: String,
    pub mode: DiffMode,
    pub side_by_side_min_width: u16,
}

fn diff_line_color(tag: ChangeTag) -> Color {
    match tag {
        ChangeTag::Delete => Color::DarkRed,
        ChangeTag::Insert => Color::DarkGreen,
        ChangeTag::Equal => Color::DarkGrey,
    }
}

fn diff_line_prefix(tag: ChangeTag) -> &'static str {
    match tag {
        ChangeTag::Delete => "- ",
        ChangeTag::Insert => "+ ",
        ChangeTag::Equal => "  ",
    }
}

pub fn unified_lines(old_text: &str, new_text: &str) -> Vec<AnyElement<'static>> {
    let diff = TextDiff::from_lines(old_text, new_text);
    diff.iter_all_changes()
        .map(|change| {
            let tag = change.tag();
            element! {
                Text(
                    content: format!("{}{}", diff_line_prefix(tag), change),
                    color: diff_line_color(tag),
                    wrap: TextWrap::NoWrap,
                )
            }
            .into()
        })
        .collect()
}

fn side_by_side_lines(old_text: &str, new_text: &str, half_width: u16) -> Vec<AnyElement<'static>> {
    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();
    let rows = old_lines.len().max(new_lines.len()).max(1);

    (0..rows)
        .map(|i| {
            let left = old_lines.get(i).copied().unwrap_or("");
            let right = new_lines.get(i).copied().unwrap_or("");
            let left_trim = crate::utils::truncate_with_ellipsis(left, half_width as usize);
            let right_trim = crate::utils::truncate_with_ellipsis(right, half_width as usize);
            element! {
                View(width: half_width.saturating_mul(2), flex_direction: FlexDirection::Row) {
                    View(width: half_width) {
                        Text(content: left_trim, color: Color::DarkRed, wrap: TextWrap::NoWrap)
                    }
                    Text(content: " │ ", color: Color::DarkGrey, wrap: TextWrap::NoWrap)
                    View(width: half_width) {
                        Text(content: right_trim, color: Color::DarkGreen, wrap: TextWrap::NoWrap)
                    }
                }
            }
            .into()
        })
        .collect()
}

/// Scrollable unified or side-by-side diff.
#[component]
pub fn DiffView(props: &DiffViewProps) -> impl Into<AnyElement<'static>> {
    let use_side_by_side = props.mode == DiffMode::SideBySide && props.width >= props.side_by_side_min_width.max(40);
    let children = if use_side_by_side {
        side_by_side_lines(&props.old_text, &props.new_text, props.width / 2)
    } else {
        unified_lines(&props.old_text, &props.new_text)
    };

    element! {
        ScrollBox(
            width: props.width,
            height: props.height,
            children: children,
        )
    }
}
