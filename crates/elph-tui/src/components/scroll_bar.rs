//! Scrollbar styling helpers and position indicator.

use iocraft::prelude::*;

/// Colors for an iocraft [`ScrollView`] scrollbar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollbarStyle {
    pub thumb_color: Option<Color>,
    pub track_color: Option<Color>,
}

impl ScrollbarStyle {
    pub fn dark() -> Self {
        Self {
            thumb_color: Some(Color::Rgb { r: 88, g: 88, b: 88 }),
            track_color: Some(Color::Rgb { r: 48, g: 48, b: 48 }),
        }
    }
}

/// Thumb length in rows for a vertical scrollbar (matches [`VerticalScrollbar`] layout).
pub fn scrollbar_thumb_rows(viewport_height: u16, content_height: u16) -> u16 {
    let vh = viewport_height as usize;
    let ch = content_height as usize;
    if vh == 0 || ch <= vh {
        return 0;
    }
    (vh * vh / ch).max(1) as u16
}

/// Props for [`ScrollIndicator`].
#[derive(Clone, Copy, Default, Props)]
pub struct ScrollIndicatorProps {
    pub offset: u32,
    pub total: u32,
    pub visible: u32,
    pub width: u16,
}

/// Props for [`VerticalScrollbar`].
#[derive(Clone, Copy, Default, Props)]
pub struct VerticalScrollbarProps {
    pub viewport_height: u16,
    pub content_height: u16,
    pub scroll_offset: u16,
    pub style: Option<ScrollbarStyle>,
}

/// One-column vertical scrollbar (iocraft [`ScrollView`] style).
#[component]
pub fn VerticalScrollbar(props: &VerticalScrollbarProps) -> impl Into<AnyElement<'static>> {
    let style = props.style.unwrap_or_else(ScrollbarStyle::dark);
    let thumb_color = style.thumb_color.unwrap_or(Color::White);
    let track_color = style.track_color.unwrap_or(Color::DarkGrey);

    let vh = props.viewport_height as usize;
    let ch = props.content_height as usize;
    let rows: Vec<_> = if vh == 0 || ch <= vh {
        Vec::new()
    } else {
        let thumb_size = scrollbar_thumb_rows(props.viewport_height, props.content_height) as usize;
        let max_off = (ch - vh) as usize;
        let thumb_pos = if max_off > 0 {
            props.scroll_offset as usize * (vh.saturating_sub(thumb_size)) / max_off
        } else {
            0
        };
        (0..vh)
            .map(|y| {
                let on_thumb = y >= thumb_pos && y < thumb_pos + thumb_size;
                element! {
                    Text(
                        content: if on_thumb { "\u{2503}" } else { "\u{2502}" },
                        color: if on_thumb { thumb_color } else { track_color },
                        wrap: TextWrap::NoWrap,
                    )
                }
            })
            .collect()
    };

    element! {
        View(width: 1, height: props.viewport_height, flex_shrink: 0f32) {
            #(rows)
        }
    }
}

/// Read-only scroll position indicator (e.g. `12/40`).
#[component]
pub fn ScrollIndicator(props: &ScrollIndicatorProps) -> impl Into<AnyElement<'static>> {
    let top = props.offset.saturating_add(1);
    let bottom = (props.offset + props.visible).min(props.total);
    let label = format!("{top}-{bottom}/{total}", total = props.total.max(1));

    element! {
        View(width: props.width, align_items: AlignItems::End) {
            Text(content: label, color: Color::DarkGrey, wrap: TextWrap::NoWrap)
        }
    }
}
