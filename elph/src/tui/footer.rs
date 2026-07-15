//! Footer status row (project + model).

use iocraft::prelude::*;

use crate::types::ThinkingLevel;

use super::labels::footer_right_label;

#[derive(Clone, Default, Props)]
pub struct FooterLeftProps {
    pub width: u16,
    pub project_label: String,
}

#[component]
pub fn FooterLeft(props: &FooterLeftProps) -> impl Into<AnyElement<'static>> {
    element! {
        View(
            width: props.width,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Start,
            padding: 0,
        ) {
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: props.project_label.clone())
        }
    }
}

#[derive(Clone, Default, Props)]
pub struct FooterRightProps {
    pub width: u16,
    pub model_label: String,
    pub thinking_level: ThinkingLevel,
    pub supports_images: bool,
}

#[component]
pub fn FooterRight(props: &FooterRightProps) -> impl Into<AnyElement<'static>> {
    let footer_right = footer_right_label(&props.model_label, props.thinking_level, props.supports_images);

    element! {
        View(
            width: props.width,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::End,
            padding: 0,
        ) {
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: footer_right)
        }
    }
}

#[derive(Clone, Default, Props)]
pub struct FooterProps {
    pub screen_width: u16,
    pub project_label: String,
    pub model_label: String,
    pub thinking_level: ThinkingLevel,
    pub supports_images: bool,
}

#[component]
pub fn Footer(props: &FooterProps) -> impl Into<AnyElement<'static>> {
    let half = props.screen_width / 2;

    element! {
        View(
            width: props.screen_width,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
        ) {
            FooterLeft(width: half, project_label: props.project_label.clone())
            FooterRight(
                width: half,
                model_label: props.model_label.clone(),
                thinking_level: props.thinking_level,
                supports_images: props.supports_images,
            )
        }
    }
}
