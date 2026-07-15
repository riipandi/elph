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

/// Thumb top row for a vertical scrollbar (matches [`VerticalScrollbar`] layout).
pub fn scrollbar_thumb_position(scroll_offset: u16, viewport_height: u16, content_height: u16) -> usize {
    let vh = viewport_height as usize;
    let ch = content_height as usize;
    if vh == 0 || ch <= vh {
        return 0;
    }
    let thumb_size = scrollbar_thumb_rows(viewport_height, content_height) as usize;
    let max_off = ch - vh;
    (scroll_offset as usize)
        .saturating_mul(vh.saturating_sub(thumb_size))
        .checked_div(max_off)
        .unwrap_or(0)
}

/// Thumb length and top row for a vertical scrollbar track.
pub fn scrollbar_thumb_geometry(viewport_height: u16, content_height: u16, scroll_offset: u16) -> (usize, usize) {
    let vh = viewport_height as usize;
    if vh == 0 {
        return (0, 0);
    }
    let ch = content_height as usize;
    if ch <= vh {
        return (0, vh);
    }
    let thumb_size = scrollbar_thumb_rows(viewport_height, content_height) as usize;
    let thumb_pos = scrollbar_thumb_position(scroll_offset, viewport_height, content_height);
    (thumb_pos, thumb_size)
}

/// Label for [`ScrollIndicator`] (e.g. `12-20/40`).
pub fn scroll_indicator_label(offset: u32, visible: u32, total: u32) -> String {
    let top = offset.saturating_add(1);
    let bottom = (offset + visible).min(total);
    format!("{top}-{bottom}/{total}", total = total.max(1))
}

/// Thumb length in rows for a vertical scrollbar (matches [`VerticalScrollbar`] layout).
pub fn scrollbar_thumb_rows(viewport_height: u16, content_height: u16) -> u16 {
    let vh = viewport_height as usize;
    let ch = content_height as usize;
    if vh == 0 {
        return 0;
    }
    if ch <= vh {
        return viewport_height;
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
    /// Full track height when taller than [`viewport_height`] (e.g. sticky chrome above scroll).
    pub track_height: Option<u16>,
    /// Track rows above the scroll thumb zone (sticky header inset).
    pub track_inset_top: Option<u16>,
}

/// Character for one vertical scrollbar cell.
pub fn scrollbar_cell_char(on_thumb: bool) -> &'static str {
    if on_thumb { "\u{2503}" } else { "\u{2502}" }
}

/// Per-row thumb flags for [`VerticalScrollbar`] (true = thumb cell).
pub fn scrollbar_thumb_row_flags(viewport_height: u16, content_height: u16, scroll_offset: u16) -> Vec<bool> {
    scrollbar_track_row_flags(viewport_height, 0, viewport_height, content_height, scroll_offset)
}

/// Per-row thumb flags on a full-height track with a non-scrolling inset at the top.
pub fn scrollbar_track_row_flags(
    track_height: u16,
    track_inset_top: u16,
    viewport_height: u16,
    content_height: u16,
    scroll_offset: u16,
) -> Vec<bool> {
    let track_h = track_height as usize;
    if track_h == 0 {
        return Vec::new();
    }
    let inset = track_inset_top.min(track_height) as usize;
    let scroll_zone = viewport_height as usize;
    let (thumb_pos, thumb_size) = scrollbar_thumb_geometry(viewport_height, content_height, scroll_offset);
    (0..track_h)
        .map(|y| {
            if y < inset {
                false
            } else {
                let local = y - inset;
                local < scroll_zone && local >= thumb_pos && local < thumb_pos + thumb_size
            }
        })
        .collect()
}

/// One-column vertical scrollbar (iocraft [`ScrollView`] style).
#[component]
pub fn VerticalScrollbar(props: &VerticalScrollbarProps) -> impl Into<AnyElement<'static>> {
    let style = props.style.unwrap_or_else(ScrollbarStyle::dark);
    let thumb_color = style.thumb_color.unwrap_or(Color::White);
    let track_color = style.track_color.unwrap_or(Color::DarkGrey);
    let track_height = props.track_height.unwrap_or(props.viewport_height);
    let track_inset = props.track_inset_top.unwrap_or(0);

    let rows: Vec<_> = scrollbar_track_row_flags(
        track_height,
        track_inset,
        props.viewport_height,
        props.content_height,
        props.scroll_offset,
    )
    .into_iter()
    .map(|on_thumb| {
        element! {
            Text(
                content: scrollbar_cell_char(on_thumb),
                color: if on_thumb { thumb_color } else { track_color },
                wrap: TextWrap::NoWrap,
            )
        }
    })
    .collect();

    element! {
        View(width: 1, height: track_height, flex_shrink: 0f32) {
            #(rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_fills_track_when_content_fits_viewport() {
        assert_eq!(scrollbar_thumb_rows(12, 4), 12);
        let flags = scrollbar_thumb_row_flags(12, 4, 0);
        assert_eq!(flags.len(), 12);
        assert!(flags.iter().all(|&on| on));
    }

    #[test]
    fn track_inset_reserves_sticky_zone_on_full_height_track() {
        let flags = scrollbar_track_row_flags(20, 4, 16, 40, 5);
        assert_eq!(flags.len(), 20);
        assert!(flags[..4].iter().all(|&on| !on));
        assert!(flags[4..].iter().any(|&on| on));
    }
}

/// Read-only scroll position indicator (e.g. `12/40`).
#[component]
pub fn ScrollIndicator(props: &ScrollIndicatorProps) -> impl Into<AnyElement<'static>> {
    let label = scroll_indicator_label(props.offset, props.visible, props.total);

    element! {
        View(width: props.width, align_items: AlignItems::End) {
            Text(content: label, color: Color::DarkGrey, wrap: TextWrap::NoWrap)
        }
    }
}
