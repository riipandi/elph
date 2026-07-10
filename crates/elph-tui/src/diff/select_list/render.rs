use crate::utils::{str_display_width, truncate_to_width_no_ellipsis};

use crate::diff::ansi::{self, styled};
use crate::diff::component::Line;

use super::SelectList;
use super::item::SelectItem;

const DEFAULT_PRIMARY_WIDTH: usize = 32;
const PRIMARY_GAP: usize = 2;
const MIN_DESCRIPTION_WIDTH: usize = 10;

impl SelectList {
    pub(super) fn display_value(item: &SelectItem) -> &str {
        if item.label.is_empty() {
            &item.value
        } else {
            &item.label
        }
    }

    pub(super) fn primary_column_width(&self) -> usize {
        let widest = self
            .filtered
            .iter()
            .map(|item| str_display_width(Self::display_value(item)) + PRIMARY_GAP)
            .max()
            .unwrap_or(DEFAULT_PRIMARY_WIDTH);
        widest.clamp(8, DEFAULT_PRIMARY_WIDTH)
    }

    pub(super) fn render_item(&self, item: &SelectItem, selected: bool, width: usize, primary_width: usize) -> Line {
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

    pub(super) fn render_lines(&mut self, width: u16) -> Vec<Line> {
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
}
