use crate::diff::component::LineComponent;
use crate::diff::overlay::{OverlayEntry, OverlayHandle, OverlayOptions};

use super::focus::{FocusState, FocusTarget};

pub(super) struct ShowOverlayResult {
    pub handle: OverlayHandle,
    pub capture_focus: bool,
}

pub(super) fn show_overlay(
    focus: &mut FocusState,
    overlays: &mut Vec<OverlayEntry>,
    term_width: u16,
    term_height: u16,
    component: Box<dyn LineComponent>,
    options: OverlayOptions,
) -> ShowOverlayResult {
    focus.focus_order_counter += 1;
    let pre_focus = match focus.focused {
        FocusTarget::Overlay(idx) => Some(idx),
        FocusTarget::None | FocusTarget::Container(_) => None,
    };
    let slot = overlays.len();
    let entry = OverlayEntry {
        component,
        options: options.clone(),
        pre_focus,
        hidden: false,
        alive: true,
        focus_order: focus.focus_order_counter,
    };
    let visible = entry.is_visible(term_width, term_height);
    overlays.push(entry);

    let capture_focus = visible && !options.non_capturing;
    ShowOverlayResult {
        handle: OverlayHandle { slot },
        capture_focus,
    }
}

pub(super) struct HideOverlayResult {
    pub removed: bool,
    pub focus_change: Option<FocusTarget>,
}

pub(super) fn hide_overlay(
    focus: &mut FocusState,
    overlays: &mut Vec<OverlayEntry>,
    term_width: u16,
    term_height: u16,
    handle: OverlayHandle,
) -> HideOverlayResult {
    let Some(entry) = overlays.get_mut(handle.slot) else {
        return HideOverlayResult {
            removed: false,
            focus_change: None,
        };
    };
    if !entry.alive {
        return HideOverlayResult {
            removed: false,
            focus_change: None,
        };
    }

    entry.alive = false;
    let pre_focus = entry.pre_focus;
    let focus_change = if focus.focused == FocusTarget::Overlay(handle.slot) {
        Some(
            focus
                .topmost_visible_overlay(overlays, term_width, term_height)
                .or(pre_focus)
                .map(FocusTarget::Overlay)
                .unwrap_or(FocusTarget::None),
        )
    } else {
        None
    };
    if overlays.iter().all(|e| !e.alive) {
        overlays.clear();
    }
    HideOverlayResult {
        removed: true,
        focus_change,
    }
}

pub(super) fn set_overlay_hidden(
    focus: &mut FocusState,
    overlays: &mut [OverlayEntry],
    term_width: u16,
    term_height: u16,
    handle: OverlayHandle,
    hidden: bool,
) -> Option<FocusTarget> {
    let entry = overlays.get_mut(handle.slot)?;
    if !entry.alive || entry.hidden == hidden {
        return None;
    }
    entry.hidden = hidden;
    let pre_focus = entry.pre_focus;
    if hidden && focus.focused == FocusTarget::Overlay(handle.slot) {
        Some(
            focus
                .topmost_visible_overlay(overlays, term_width, term_height)
                .or(pre_focus)
                .map(FocusTarget::Overlay)
                .unwrap_or(FocusTarget::None),
        )
    } else if !hidden && !entry.options.non_capturing {
        focus.focus_order_counter += 1;
        entry.focus_order = focus.focus_order_counter;
        Some(FocusTarget::Overlay(handle.slot))
    } else {
        None
    }
}

pub(super) fn focus_overlay(
    focus: &mut FocusState,
    overlays: &mut [OverlayEntry],
    term_width: u16,
    term_height: u16,
    handle: OverlayHandle,
) -> Option<FocusTarget> {
    let visible = overlays
        .get(handle.slot)
        .is_some_and(|e| e.alive && e.is_visible(term_width, term_height));
    if !visible {
        return None;
    }
    focus.focus_order_counter += 1;
    if let Some(entry) = overlays.get_mut(handle.slot) {
        entry.focus_order = focus.focus_order_counter;
    }
    Some(FocusTarget::Overlay(handle.slot))
}
