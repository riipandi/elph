//! Slim bordered palette shell with a single top-left label.

use iocraft::prelude::*;

use super::chrome::PaletteCardChrome;

#[derive(Default, Props)]
pub struct PaletteCardFrameProps {
    pub chrome: PaletteCardChrome,
    pub children: Vec<AnyElement<'static>>,
}

#[component]
pub fn PaletteCardFrame(props: &mut PaletteCardFrameProps) -> impl Into<AnyElement<'static>> {
    let chrome = &props.chrome;
    let children = std::mem::take(&mut props.children);
    let title = format!(" {} ", chrome.title);

    element! {
        View(
            width: chrome.card_width,
            border_style: BorderStyle::Round,
            border_color: chrome.border_color,
            background_color: chrome.background,
            padding_top: 0u16,
            padding_bottom: 0u16,
            padding_left: 1u16,
            padding_right: 1u16,
            gap: 0u16,
            flex_direction: FlexDirection::Column,
            position: Position::Relative,
        ) {
            View(
                position: Position::Absolute,
                top: 0,
                left: 1,
                margin_top: -1,
                background_color: chrome.background,
            ) {
                Text(
                    content: title,
                    color: chrome.title_color,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                )
            }
            #(children)
        }
    }
}
