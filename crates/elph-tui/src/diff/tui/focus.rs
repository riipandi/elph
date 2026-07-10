use crate::diff::component::{Container, InputResult};
use crate::diff::overlay::OverlayEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FocusTarget {
    None,
    Container(usize),
    Overlay(usize),
}

pub(super) struct FocusState {
    pub focused: FocusTarget,
    pub focus_order_counter: u64,
}

impl FocusState {
    pub fn new() -> Self {
        Self {
            focused: FocusTarget::None,
            focus_order_counter: 0,
        }
    }

    pub fn dispatch_input(&self, data: &str, container: &mut Container, overlays: &mut [OverlayEntry]) -> bool {
        if let FocusTarget::Overlay(idx) = self.focused
            && let Some(entry) = overlays.get_mut(idx)
            && entry.alive
            && !entry.hidden
        {
            return entry.component.handle_input(data) == InputResult::Consumed;
        }
        if let FocusTarget::Container(idx) = self.focused
            && let Some(child) = container.child_mut(idx)
        {
            return child.handle_input(data) == InputResult::Consumed;
        }
        false
    }

    pub fn set_focus(&mut self, target: FocusTarget, container: &mut Container, overlays: &mut [OverlayEntry]) {
        self.clear_focus(container, overlays);
        self.focused = target;

        match self.focused {
            FocusTarget::Overlay(idx) => {
                if let Some(entry) = overlays.get_mut(idx) {
                    entry.component.set_focused(true);
                }
            }
            FocusTarget::Container(idx) => {
                if let Some(child) = container.child_mut(idx) {
                    child.set_focused(true);
                }
            }
            FocusTarget::None => {}
        }
    }

    pub fn clear_focus(&mut self, container: &mut Container, overlays: &mut [OverlayEntry]) {
        match self.focused {
            FocusTarget::Overlay(idx) => {
                if let Some(entry) = overlays.get_mut(idx) {
                    entry.component.set_focused(false);
                }
            }
            FocusTarget::Container(idx) => {
                if let Some(child) = container.child_mut(idx) {
                    child.set_focused(false);
                }
            }
            FocusTarget::None => {}
        }
    }

    pub fn topmost_visible_overlay(
        &self,
        overlays: &[OverlayEntry],
        term_width: u16,
        term_height: u16,
    ) -> Option<usize> {
        overlays
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.alive
                    && !entry.hidden
                    && !entry.options.non_capturing
                    && entry.is_visible(term_width, term_height)
            })
            .max_by_key(|(_, entry)| entry.focus_order)
            .map(|(idx, _)| idx)
    }
}
