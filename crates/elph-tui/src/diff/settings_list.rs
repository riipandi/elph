use crate::utils::truncate_to_width_no_ellipsis;

use super::ansi::{self, styled};
use super::component::{InputResult, Line, LineComponent};
use super::keys;

/// One row in a settings panel.
#[derive(Debug, Clone)]
pub struct SettingItem {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub current_value: String,
    pub values: Option<Vec<String>>,
}

impl SettingItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>, current_value: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            current_value: current_value.into(),
            values: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_values(mut self, values: Vec<String>) -> Self {
        self.values = Some(values);
        self
    }
}

/// ANSI palette for [`SettingsList`].
#[derive(Debug, Clone, Copy)]
pub struct SettingsListTheme {
    pub label: u8,
    pub value: u8,
    pub description: u8,
    pub cursor: u8,
    pub hint: u8,
}

impl SettingsListTheme {
    pub fn dark() -> Self {
        Self {
            label: 252,
            value: 51,
            description: 240,
            cursor: 51,
            hint: 245,
        }
    }
}

pub type SettingsChangeCallback = Box<dyn FnMut(&str, &str)>;
pub type SettingsCancelCallback = Box<dyn FnMut()>;

/// Settings panel with value cycling (pi-tui `SettingsList`).
pub struct SettingsList {
    items: Vec<SettingItem>,
    selected_index: usize,
    max_visible: usize,
    theme: SettingsListTheme,
    focused: bool,
    pub on_change: Option<SettingsChangeCallback>,
    pub on_cancel: Option<SettingsCancelCallback>,
}

impl SettingsList {
    pub fn new(items: Vec<SettingItem>, max_visible: usize, theme: SettingsListTheme) -> Self {
        Self {
            items,
            selected_index: 0,
            max_visible: max_visible.max(1),
            theme,
            focused: false,
            on_change: None,
            on_cancel: None,
        }
    }

    pub fn set_items(&mut self, items: Vec<SettingItem>) {
        self.items = items;
        self.selected_index = 0;
        self.invalidate();
    }

    pub fn update_value(&mut self, id: &str, new_value: impl Into<String>) {
        let value = new_value.into();
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.current_value = value;
            self.invalidate();
        }
    }

    pub fn selected_item(&self) -> Option<&SettingItem> {
        self.items.get(self.selected_index)
    }

    fn cycle_value(&mut self, index: usize) {
        let Some(item) = self.items.get_mut(index) else {
            return;
        };
        let Some(values) = item.values.as_ref() else {
            return;
        };
        if values.is_empty() {
            return;
        }
        let current_idx = values.iter().position(|v| v == &item.current_value).unwrap_or(0);
        let next = (current_idx + 1) % values.len();
        item.current_value = values[next].clone();
        let id = item.id.clone();
        let value = item.current_value.clone();
        if let Some(cb) = &mut self.on_change {
            cb(&id, &value);
        }
        self.invalidate();
    }

    fn render_row(&self, item: &SettingItem, selected: bool, width: usize) -> Line {
        let cursor = if selected { "› " } else { "  " };
        let label = truncate_to_width_no_ellipsis(&item.label, width.saturating_sub(20).max(8));
        let value = truncate_to_width_no_ellipsis(&item.current_value, 16);
        let label_styled = if selected {
            styled(&ansi::fg(self.theme.label), &format!("{cursor}{label}"))
        } else {
            format!("{cursor}{label}")
        };
        let value_styled = styled(&ansi::fg(self.theme.value), &format!("  {value}"));
        truncate_to_width_no_ellipsis(&format!("{label_styled}{value_styled}"), width)
    }

    fn invalidate(&mut self) {}
}

impl LineComponent for SettingsList {
    fn render(&mut self, width: u16) -> Vec<Line> {
        let width = width.max(1) as usize;
        if self.items.is_empty() {
            return vec![styled(&ansi::fg(self.theme.hint), "  No settings")];
        }

        let start = self
            .selected_index
            .saturating_sub(self.max_visible / 2)
            .min(self.items.len().saturating_sub(self.max_visible));
        let end = (start + self.max_visible).min(self.items.len());

        let mut lines = Vec::new();
        for (i, item) in self.items[start..end].iter().enumerate() {
            let index = start + i;
            lines.push(self.render_row(item, index == self.selected_index, width));
            if index == self.selected_index {
                if let Some(desc) = &item.description {
                    let hint = truncate_to_width_no_ellipsis(desc, width.saturating_sub(4));
                    lines.push(styled(&ansi::fg(self.theme.description), &format!("    {hint}")));
                }
                if item.values.is_some() {
                    lines.push(styled(&ansi::fg(self.theme.hint), "    Enter/Space: cycle value"));
                }
            }
        }
        lines
    }

    fn invalidate(&mut self) {}

    fn handle_input(&mut self, data: &str) -> InputResult {
        if !self.focused || self.items.is_empty() {
            return InputResult::Ignored;
        }

        if keys::is_up(data) {
            self.selected_index = if self.selected_index == 0 {
                self.items.len() - 1
            } else {
                self.selected_index - 1
            };
            return InputResult::Consumed;
        }

        if keys::is_down(data) {
            self.selected_index = if self.selected_index + 1 >= self.items.len() {
                0
            } else {
                self.selected_index + 1
            };
            return InputResult::Consumed;
        }

        if keys::is_enter(data) || data == " " {
            self.cycle_value(self.selected_index);
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
    fn cycles_value_on_enter() {
        let mut list = SettingsList::new(
            vec![SettingItem::new("theme", "Theme", "dark").with_values(vec!["dark".into(), "light".into()])],
            5,
            SettingsListTheme::dark(),
        );
        list.set_focused(true);
        list.handle_input("\r");
        assert_eq!(list.selected_item().unwrap().current_value, "light");
    }
}
