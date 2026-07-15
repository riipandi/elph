//! Styled text display (OpenTUI Text analogue).

use iocraft::prelude::*;

/// Props for [`StyledText`].
#[derive(Clone, Default, Props)]
pub struct StyledTextProps {
    pub content: String,
    pub color: Option<Color>,
    pub weight: Weight,
    pub wrap: TextWrap,
    pub align: TextAlign,
    pub italic: bool,
}

/// Display styled text content.
#[component]
pub fn StyledText(props: &StyledTextProps) -> impl Into<AnyElement<'static>> {
    element! {
        Text(
            content: props.content.clone(),
            color: props.color.unwrap_or(Color::Reset),
            weight: props.weight,
            wrap: props.wrap,
            align: props.align,
            italic: props.italic,
        )
    }
}
