//! Footer status row (project + model).

use iocraft::prelude::*;

use crate::tui::chrome::{chrome_half_width, fit_footer_left, fit_footer_left_from_line, fit_footer_right};
use crate::tui::labels::GitFooterInfo;
use crate::types::ThinkingLevel;

#[derive(Clone, Default, Props)]
pub struct FooterLeftProps {
    pub width: u16,
    pub project_name: String,
    pub project_line: String,
    pub git: Option<GitFooterInfo>,
    pub turn: u32,
}

#[component]
pub fn FooterLeft(props: &FooterLeftProps) -> impl Into<AnyElement<'static>> {
    let max_width = props.width.max(1) as usize;
    let label = if props.project_line.is_empty() {
        fit_footer_left(&props.project_name, props.git.as_ref(), props.turn, max_width)
    } else {
        fit_footer_left_from_line(&props.project_line, props.turn, max_width)
    };

    element! {
        View(
            width: props.width,
            flex_shrink: 0f32,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Start,
            padding: 0,
        ) {
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: label)
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
    let label = fit_footer_right(
        &props.model_label,
        props.thinking_level,
        props.supports_images,
        props.width.max(1) as usize,
    );

    element! {
        View(
            width: props.width,
            flex_shrink: 0f32,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::End,
            padding: 0,
        ) {
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: label)
        }
    }
}

#[derive(Clone, Default, Props)]
pub struct FooterProps {
    pub screen_width: u16,
    /// Preformatted project/git line from bootstrap (shown before live chrome refresh).
    pub project_line: String,
    pub project_name: String,
    pub git: Option<GitFooterInfo>,
    pub turn: u32,
    pub model_label: String,
    pub thinking_level: ThinkingLevel,
    pub supports_images: bool,
    /// Bumped when chrome stats/git refresh so footer repaints eagerly.
    pub chrome_revision: u64,
}

#[component]
pub fn Footer(props: &FooterProps) -> impl Into<AnyElement<'static>> {
    let _chrome_revision = props.chrome_revision;
    let half = chrome_half_width(props.screen_width.max(1)) as u16;

    element! {
        View(
            width: props.screen_width.max(1),
            flex_shrink: 0f32,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
        ) {
            FooterLeft(
                width: half,
                project_name: props.project_name.clone(),
                project_line: props.project_line.clone(),
                git: props.git.clone(),
                turn: props.turn,
            )
            FooterRight(
                width: half,
                model_label: props.model_label.clone(),
                thinking_level: props.thinking_level,
                supports_images: props.supports_images,
            )
        }
    }
}
