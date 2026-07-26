//! Bounded scrollable region (OpenTUI scroll container analogue).

use super::scroll_bar::ScrollbarStyle;
use super::theme::{UiTheme, resolve_ui_theme};
use iocraft::prelude::*;

/// Bottom-pinned line offset for a [`ScrollView`] with `auto_scroll: true`.
pub fn scroll_view_max_offset(content_height: u16, viewport_height: u16) -> i32 {
    (content_height as i32).saturating_sub(viewport_height as i32).max(0)
}

/// Target scroll offset after scrolling up while pinned to the bottom.
pub fn scroll_view_pinned_up_offset(content_height: u16, viewport_height: u16, step: i32) -> i32 {
    let step = step.max(1);
    let max = scroll_view_max_offset(content_height, viewport_height);
    max.saturating_sub(step)
}

/// Whether scrolling down should re-pin to the bottom.
pub fn scroll_view_down_reaches_bottom(
    scroll_offset: i32,
    content_height: u16,
    viewport_height: u16,
    step: i32,
) -> bool {
    let step = step.max(1);
    let max = scroll_view_max_offset(content_height, viewport_height);
    scroll_offset + step >= max
}

/// Scroll up by `step` lines without jumping when pinned to the bottom via auto-scroll.
pub fn scroll_view_up(handle: &mut ScrollViewHandle, step: i32) {
    if handle.is_auto_scroll_pinned() {
        handle.scroll_to(scroll_view_pinned_up_offset(
            handle.content_height(),
            handle.viewport_height(),
            step,
        ));
    } else {
        handle.scroll_by(-step.max(1));
    }
}

/// Scroll down by `step` lines; re-pins auto-scroll when the bottom is reached.
pub fn scroll_view_down(handle: &mut ScrollViewHandle, step: i32) {
    let step = step.max(1);
    if handle.is_auto_scroll_pinned() {
        return;
    }
    if scroll_view_down_reaches_bottom(handle.scroll_offset(), handle.content_height(), handle.viewport_height(), step)
    {
        handle.scroll_to_bottom();
    } else {
        handle.scroll_by(step);
    }
}

/// Props for [`ScrollBox`].
#[derive(Default, Props)]
pub struct ScrollBoxProps<'a> {
    pub width: u16,
    pub height: u16,
    pub auto_scroll: bool,
    pub keyboard_scroll: bool,
    pub scroll_step: u16,
    pub scrollbar: bool,
    pub scrollbar_style: Option<ScrollbarStyle>,
    pub theme: Option<UiTheme>,
    /// Optional handle to read scroll offset for linked [`VerticalScrollbar`] / [`ScrollIndicator`].
    pub handle: Option<Ref<ScrollViewHandle>>,
    /// Suppress the border and default background so the box blends into a parent card.
    pub no_border: bool,
    pub children: Vec<AnyElement<'a>>,
}

/// Clipped viewport with an inner [`ScrollView`].
#[component]
pub fn ScrollBox<'a>(props: &mut ScrollBoxProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let style = props.scrollbar_style.unwrap_or_else(|| theme.scrollbar_style());
    let children = std::mem::take(&mut props.children);
    let builtin_scrollbar = props.scrollbar && props.handle.is_none();
    let border_style = if props.no_border {
        BorderStyle::None
    } else {
        BorderStyle::Single
    };
    let border_color = if props.no_border { Color::Reset } else { theme.border };
    let bg = if props.no_border {
        Color::Reset
    } else {
        theme.list_surface()
    };

    element! {
        View(
            width: props.width,
            height: props.height,
            overflow: Overflow::Hidden,
            border_style: border_style,
            border_color: border_color,
            background_color: bg,
        ) {
            View(width: 100pct, height: 100pct, overflow: Overflow::Hidden) {
                ScrollView(
                    handle: props.handle,
                    auto_scroll: props.auto_scroll,
                    keyboard_scroll: Some(props.keyboard_scroll),
                    scroll_step: if props.scroll_step == 0 {
                        None
                    } else {
                        Some(props.scroll_step)
                    },
                    scrollbar: Some(builtin_scrollbar),
                    scrollbar_thumb_color: style.thumb_color,
                    scrollbar_track_color: style.track_color,
                ) {
                    #(children)
                }
            }
        }
    }
}
