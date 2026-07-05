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
