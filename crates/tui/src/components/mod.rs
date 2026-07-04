mod label;

pub use label::{Label, LabelProps};

use iocraft::prelude::*;

pub fn frame<'a>(theme: crate::theme::Theme, children: Vec<AnyElement<'a>>) -> Element<'a, View> {
    element! {
        View(
            border_style: BorderStyle::Round,
            border_color: theme.frame_border,
            padding_top: 2,
            padding_bottom: 2,
            padding_left: 8,
            padding_right: 8,
        ) {
            #(children.into_iter())
        }
    }
}
