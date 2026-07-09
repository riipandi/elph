//! Owly startup banner rendered inside the iocraft layout.

use std::path::Path;

use elph_tui::Theme;
use iocraft::prelude::*;

use crate::cli::truncate_path_for_display;

use super::chrome::{H_INSET, SECTION_PAD, subtle_border};

#[derive(Default, Props)]
pub struct OwlyBannerProps {
    pub provider: String,
    pub model: String,
    pub directory: String,
    pub version: String,
    pub theme: Theme,
}

#[component]
pub fn OwlyBanner(props: &OwlyBannerProps) -> impl Into<AnyElement<'static>> {
    let palette = props.theme;
    let title = format!(">_ Owly v{} agent docs for codebases", props.version);

    element! {
        View(
            flex_shrink: 0.0,
            width: 100pct,
            flex_direction: FlexDirection::Column,
            padding_left: H_INSET,
            padding_right: H_INSET,
            padding_top: SECTION_PAD,
            padding_bottom: SECTION_PAD,
            gap: Gap::Length(1),
            border_style: BorderStyle::Single,
            border_color: subtle_border(palette),
        ) {
            Text(content: title, color: Color::Cyan, weight: Weight::Bold)
            Text(content: format!("provider: {}", props.provider), color: Color::Green)
            Text(content: format!("model: {}", props.model), color: Color::Green)
            Text(content: format!("directory: {}", props.directory), color: palette.muted)
        }
    }
}

pub fn directory_display(cwd: &Path) -> String {
    truncate_path_for_display(cwd, 48)
}
