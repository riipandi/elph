mod item;
mod render;

use crate::diff::component::{InputResult, LineComponent};
use crate::diff::fuzzy::fuzzy_filter;
use crate::diff::keys;

pub use item::{SelectCallback, SelectChangeCallback, SelectItem, SelectListTheme};

/// Scrollable fuzzy-filtered list.
pub struct SelectList {
    pub(super) items: Vec<SelectItem>,
    pub(super) filtered: Vec<SelectItem>,
    pub(super) selected_index: usize,
    pub(super) max_visible: usize,
    pub(super) filter: String,
    pub(super) theme: SelectListTheme,
    pub(super) focused: bool,
    pub on_select: Option<SelectCallback>,
    pub on_cancel: Option<Box<dyn FnMut()>>,
    pub on_selection_change: Option<SelectChangeCallback>,
}

impl SelectList {
    pub fn new(items: Vec<SelectItem>, max_visible: usize, theme: SelectListTheme) -> Self {
        let filtered = items.clone();
        Self {
            items,
            filtered,
            selected_index: 0,
            max_visible: max_visible.max(1),
            filter: String::new(),
            theme,
            focused: false,
            on_select: None,
            on_cancel: None,
            on_selection_change: None,
        }
    }

    pub fn set_items(&mut self, items: Vec<SelectItem>) {
        self.items = items;
        self.apply_filter();
    }

    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into();
        self.apply_filter();
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn set_selected_index(&mut self, index: usize) {
        if self.filtered.is_empty() {
            self.selected_index = 0;
            return;
        }
        self.selected_index = index.min(self.filtered.len() - 1);
    }

    pub fn selected_item(&self) -> Option<&SelectItem> {
        self.filtered.get(self.selected_index)
    }

    fn apply_filter(&mut self) {
        self.filtered = fuzzy_filter(&self.items, &self.filter, |item| {
            let mut text = item.label.clone();
            if !item.value.is_empty() {
                text.push(' ');
                text.push_str(&item.value);
            }
            if let Some(desc) = &item.description {
                text.push(' ');
                text.push_str(desc);
            }
            text
        });
        self.selected_index = 0;
        self.invalidate();
    }

    fn notify_selection_change(&mut self) {
        let item = self.selected_item().cloned();
        if let (Some(item), Some(cb)) = (item, &mut self.on_selection_change) {
            cb(&item);
        }
    }
}

impl LineComponent for SelectList {
    fn render(&mut self, width: u16) -> Vec<crate::diff::component::Line> {
        self.render_lines(width)
    }

    fn invalidate(&mut self) {}

    fn handle_input(&mut self, data: &str) -> InputResult {
        if !self.focused || self.filtered.is_empty() {
            return InputResult::Ignored;
        }

        if keys::is_up(data) {
            self.selected_index = if self.selected_index == 0 {
                self.filtered.len() - 1
            } else {
                self.selected_index - 1
            };
            self.notify_selection_change();
            return InputResult::Consumed;
        }

        if keys::is_down(data) {
            self.selected_index = if self.selected_index + 1 >= self.filtered.len() {
                0
            } else {
                self.selected_index + 1
            };
            self.notify_selection_change();
            return InputResult::Consumed;
        }

        if keys::is_enter(data) {
            let item = self.selected_item().cloned();
            if let (Some(item), Some(cb)) = (item, &mut self.on_select) {
                cb(&item);
            }
            return InputResult::Consumed;
        }

        if keys::is_cancel(data) {
            if let Some(cb) = &mut self.on_cancel {
                cb();
            }
            return InputResult::Consumed;
        }

        InputResult::Ignored
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn is_focused(&self) -> bool {
        self.focused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_filter_narrows_items() {
        let mut list = SelectList::new(
            vec![
                SelectItem::new("git-status", "Git Status"),
                SelectItem::new("cargo-test", "Cargo Test"),
            ],
            5,
            SelectListTheme::dark(),
        );
        list.set_filter("git");
        let lines = list.render(40);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("Git Status"));
    }
}
