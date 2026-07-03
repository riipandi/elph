use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct LabelProps {
    pub content: String,
    pub color: Option<Color>,
}

#[component]
pub fn Label(props: &LabelProps) -> impl Into<AnyElement<'static>> {
    element! {
        Text(
            color: props.color,
            content: &props.content,
        )
    }
}

pub fn frame<'a>(children: Vec<AnyElement<'a>>) -> Element<'a, View> {
    element! {
        View(
            border_style: BorderStyle::Round,
            border_color: Color::Blue,
            padding_top: 2,
            padding_bottom: 2,
            padding_left: 8,
            padding_right: 8,
        ) {
            #(children.into_iter())
        }
    }
}
