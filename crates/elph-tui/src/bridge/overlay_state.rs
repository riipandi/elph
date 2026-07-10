//! Overlay stack for bridging diff components into SLT.

use crate::diff::{
    InputResult, Line, LineComponent, OverlayEntry, OverlayHandle, OverlayOptions, composite_overlays, resolve_layout,
};

/// Mutable slot holding a diff [`LineComponent`] overlay.
pub struct OverlaySlot {
    pub component: Box<dyn LineComponent>,
    pub options: OverlayOptions,
    focused: bool,
}

impl OverlaySlot {
    pub fn new(component: Box<dyn LineComponent>, options: OverlayOptions) -> Self {
        Self {
            component,
            options,
            focused: true,
        }
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.component.set_focused(focused);
    }
}

/// Stack of overlays with focus and input routing.
#[derive(Default)]
pub struct OverlayStack {
    entries: Vec<OverlayEntry>,
    focus_order_counter: u64,
}

impl OverlayStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.iter().all(|e| !e.alive)
    }

    pub fn has_visible_overlay(&self, term_width: u16, term_height: u16) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.is_visible(term_width, term_height))
    }

    /// Mount an overlay and focus it unless `non_capturing`.
    pub fn show(&mut self, slot: OverlaySlot) -> OverlayHandle {
        self.focus_order_counter += 1;
        let index = self.entries.len();
        let capturing = !slot.options.non_capturing;
        let entry = OverlayEntry {
            component: slot.component,
            options: slot.options,
            pre_focus: None,
            hidden: false,
            alive: true,
            focus_order: self.focus_order_counter,
        };
        self.entries.push(entry);
        if capturing {
            self.set_focus(index);
        }
        OverlayHandle { slot: index }
    }

    pub fn hide(&mut self, handle: OverlayHandle) -> bool {
        let Some(entry) = self.entries.get_mut(handle.slot) else {
            return false;
        };
        if !entry.alive {
            return false;
        }
        entry.alive = false;
        entry.component.set_focused(false);
        self.purge_dead_entries();
        true
    }

    /// Drops tombstoned overlays once none remain alive, freeing component memory.
    fn purge_dead_entries(&mut self) {
        if self.entries.iter().all(|e| !e.alive) {
            self.entries.clear();
        }
    }

    pub fn set_hidden(&mut self, handle: OverlayHandle, hidden: bool) {
        let Some(entry) = self.entries.get_mut(handle.slot) else {
            return;
        };
        if !entry.alive {
            return;
        }
        entry.hidden = hidden;
        if hidden {
            entry.component.set_focused(false);
        }
    }

    fn set_focus(&mut self, index: usize) {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            entry.component.set_focused(i == index && entry.alive && !entry.hidden);
        }
    }

    fn focused_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.alive && !e.hidden && !e.options.non_capturing)
            .max_by_key(|(_, e)| e.focus_order)
            .map(|(i, _)| i)
    }

    /// Routes raw terminal input to the focused overlay component.
    pub fn handle_input(&mut self, data: &str) -> bool {
        let Some(idx) = self.focused_index() else {
            return false;
        };
        let Some(entry) = self.entries.get_mut(idx) else {
            return false;
        };
        entry.component.handle_input(data) == InputResult::Consumed
    }

    /// Renders the top overlay on a dimmed background placeholder.
    pub fn render(&mut self, term_width: u16, term_height: u16) -> Vec<Line> {
        let width = term_width.max(1);
        let height = term_height.max(1);
        let base: Vec<Line> = std::iter::repeat_n(String::new(), height as usize).collect();
        composite_overlays(base, &mut self.entries[..], width, height)
    }

    /// Renders only the focused overlay lines (for compact SLT embedding).
    pub fn render_focused(&mut self, term_width: u16, term_height: u16) -> Vec<Line> {
        let Some(idx) = self.focused_index() else {
            return Vec::new();
        };
        let entry = &mut self.entries[idx];
        let layout = resolve_layout(&entry.options, 1, term_width, term_height);
        let width = layout.map(|l| l.width).unwrap_or(term_width.max(1));
        entry.component.render(width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{SelectItem, SelectList, SelectListTheme, Text};

    #[test]
    fn stack_shows_and_hides_overlay() {
        let mut stack = OverlayStack::new();
        let list = SelectList::new(vec![SelectItem::new("a", "Alpha")], 4, SelectListTheme::dark());
        let handle = stack.show(OverlaySlot::new(Box::new(list), OverlayOptions::default()));
        assert!(!stack.is_empty());
        stack.hide(handle);
        assert!(stack.is_empty());
    }

    #[test]
    fn stack_purges_dead_entries_when_all_hidden() {
        let mut stack = OverlayStack::new();
        let h1 = stack.show(OverlaySlot::new(Box::new(Text::new("one")), OverlayOptions::default()));
        let h2 = stack.show(OverlaySlot::new(Box::new(Text::new("two")), OverlayOptions::default()));
        assert!(!stack.is_empty());
        stack.hide(h1);
        assert!(!stack.is_empty());
        stack.hide(h2);
        assert!(stack.is_empty());
    }

    #[test]
    fn stack_renders_background_with_text() {
        let mut stack = OverlayStack::new();
        stack.show(OverlaySlot::new(
            Box::new(Text::new("Hello")),
            OverlayOptions::default(),
        ));
        let lines = stack.render_focused(40, 10);
        assert!(!lines.is_empty());
    }
}
