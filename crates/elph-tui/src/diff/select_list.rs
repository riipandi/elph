use crate::utils::{str_display_width, truncate_to_width_no_ellipsis};

use super::ansi::{self, styled};
use super::component::{InputResult, Line, LineComponent};
use super::fuzzy::fuzzy_filter;
use super::keys;

const DEFAULT_PRIMARY_WIDTH: usize = 32;
const PRIMARY_GAP: usize = 2;
const MIN_DESCRIPTION_WIDTH: usize = 10;

/// One selectable row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl SelectItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// ANSI styling for [`SelectList`].
#[derive(Debug, Clone, Copy)]
pub struct SelectListTheme {
    pub selected: u8,
    pub description: u8,
    pub scroll_info: u8,
    pub no_match: u8,
}

impl SelectListTheme {
    pub fn dark() -> Self {
        Self {
            selected: 51,
            description: 240,
            scroll_info: 245,
            no_match: 240,
        }
    }
}

/// Callback when a list item is selected.
pub type SelectCallback = Box<dyn FnMut(&SelectItem)>;
/// Callback when list selection changes.
pub type SelectChangeCallback = Box<dyn FnMut(&SelectItem)>;

/// Scrollable fuzzy-filtered list.
pub struct SelectList {
    items: Vec<SelectItem>,
    filtered: Vec<SelectItem>,
    selected_index: usize,
    max_visible: usize,
    filter: String,
    theme: SelectListTheme,
    focused: bool,
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

    fn display_value(item: &SelectItem) -> &str {
        if item.label.is_empty() {
            &item.value
        } else {
            &item.label
        }
    }

    fn primary_column_width(&self) -> usize {
        let widest = self
            .filtered
            .iter()
            .map(|item| str_display_width(Self::display_value(item)) + PRIMARY_GAP)
            .max()
            .unwrap_or(DEFAULT_PRIMARY_WIDTH);
        widest.clamp(8, DEFAULT_PRIMARY_WIDTH)
    }

    fn render_item(&self, item: &SelectItem, selected: bool, width: usize, primary_width: usize) -> Line {
        let prefix = if selected { "→ " } else { "  " };
        let prefix_width = str_display_width(prefix);
        let description = item
            .description
            .as_deref()
            .map(|d| d.replace(['\r', '\n'], " ").trim().to_string())
            .filter(|d| !d.is_empty());

        if let Some(desc) = description
            && width > 40
        {
            let effective_primary = primary_width.min(width.saturating_sub(prefix_width + 4));
            let max_primary = effective_primary.saturating_sub(PRIMARY_GAP).max(1);
            let value = truncate_to_width_no_ellipsis(Self::display_value(item), max_primary);
            let value_width = str_display_width(&value);
            let spacing = " ".repeat(effective_primary.saturating_sub(value_width).max(1));
            let remaining = width.saturating_sub(prefix_width + value_width + spacing.len() + 2);
            if remaining > MIN_DESCRIPTION_WIDTH {
                let truncated_desc = truncate_to_width_no_ellipsis(&desc, remaining);
                if selected {
                    return styled(
                        &ansi::fg(self.theme.selected),
                        &format!("{prefix}{value}{spacing}{truncated_desc}"),
                    );
                }
                let desc_styled = styled(&ansi::fg(self.theme.description), &format!("{spacing}{truncated_desc}"));
                return format!("{prefix}{value}{desc_styled}");
            }
        }

        let max_width = width.saturating_sub(prefix_width + 2);
        let value = truncate_to_width_no_ellipsis(Self::display_value(item), max_width);
        if selected {
            styled(&ansi::fg(self.theme.selected), &format!("{prefix}{value}"))
        } else {
            format!("{prefix}{value}")
        }
    }
}

impl LineComponent for SelectList {
    fn render(&mut self, width: u16) -> Vec<Line> {
        let width = width.max(1) as usize;
        if self.filtered.is_empty() {
            return vec![styled(&ansi::fg(self.theme.no_match), "  No matching items")];
        }

        let primary_width = self.primary_column_width();
        let start = self
            .selected_index
            .saturating_sub(self.max_visible / 2)
            .min(self.filtered.len().saturating_sub(self.max_visible));
        let end = (start + self.max_visible).min(self.filtered.len());

        let mut lines = Vec::new();
        for (i, item) in self.filtered[start..end].iter().enumerate() {
            let index = start + i;
            lines.push(self.render_item(item, index == self.selected_index, width, primary_width));
        }

        if start > 0 || end < self.filtered.len() {
            let scroll = format!("  ({}/{})", self.selected_index + 1, self.filtered.len());
            lines.push(styled(
                &ansi::fg(self.theme.scroll_info),
                &truncate_to_width_no_ellipsis(&scroll, width.saturating_sub(2)),
            ));
        }

        lines
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
