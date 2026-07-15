//! Line number gutter for code blocks.

use iocraft::prelude::*;

/// Props for [`LineNumbers`].
#[derive(Clone, Default, Props)]
pub struct LineNumbersProps {
    pub line_count: usize,
    pub start_line: usize,
    pub gutter_width: u16,
    pub color: Option<Color>,
}

/// Right-aligned line numbers column.
#[component]
pub fn LineNumbers(props: &LineNumbersProps) -> impl Into<AnyElement<'static>> {
    let color = props.color.unwrap_or(Color::DarkGrey);
    let mut lines = Vec::new();
    for i in 0..props.line_count {
        let num = props.start_line + i + 1;
        lines.push(element! {
            Text(content: format!("{num:>width$}", width = props.gutter_width as usize - 1), color, wrap: TextWrap::NoWrap)
        });
    }

    element! {
        View(
            width: props.gutter_width,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::End,
            padding_right: 1,
        ) {
            #(lines)
        }
    }
}
