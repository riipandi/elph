//! Row metrics for slash palette command + wrapped description columns.

use elph_tui::utils::wrap_text;

/// Command name column width (prefix + `/name`) for aligned descriptions.
pub const CMD_COLUMN_CHARS: usize = 18;

/// Space between command column and description column.
pub const CMD_DESC_GAP_COLS: u16 = 1;

/// Maximum wrapped description lines per palette item.
pub const MAX_DESC_WRAP_LINES: usize = 3;

/// Outer card width — matches the editor chrome (`screen_width`).
pub fn palette_card_width(screen_width: u16) -> u16 {
    screen_width.max(20)
}

/// List content width inside the card frame (editor inner width minus scrollbar column).
pub fn palette_list_width(screen_width: u16) -> u16 {
    screen_width.saturating_sub(3).max(20)
}

/// Description column width in terminal cells.
pub fn palette_desc_width(list_width: u16) -> usize {
    list_width
        .saturating_sub(CMD_COLUMN_CHARS as u16 + CMD_DESC_GAP_COLS)
        .max(1) as usize
}

/// Wrapped description lines for one palette row (capped).
pub fn wrap_palette_description(description: &str, list_width: u16) -> Vec<String> {
    let width = palette_desc_width(list_width);
    let mut lines = wrap_text(description, width);
    if lines.len() > MAX_DESC_WRAP_LINES {
        lines.truncate(MAX_DESC_WRAP_LINES);
        if let Some(last) = lines.last_mut() {
            *last = truncate_line_ellipsis(last, width);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Terminal row count for one palette item.
pub fn palette_item_row_lines(description: &str, list_width: u16) -> u16 {
    wrap_palette_description(description, list_width).len().max(1) as u16
}

/// Sum of terminal rows for a slice of options (capped at `viewport_cap`).
pub fn visible_terminal_rows(
    options: &[elph_tui::SelectOption],
    window_start: usize,
    item_cap: usize,
    list_width: u16,
    viewport_cap: usize,
) -> u16 {
    let mut total = 0usize;
    for opt in options.iter().skip(window_start).take(item_cap) {
        total += palette_item_row_lines(&opt.description, list_width) as usize;
        if total >= viewport_cap {
            return viewport_cap.max(1) as u16;
        }
    }
    total.max(1) as u16
}

fn truncate_line_ellipsis(line: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = line.chars().count();
    if char_count <= max_chars {
        return line.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = line.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_width_reserves_scrollbar_column() {
        assert_eq!(palette_list_width(80), 77);
        assert_eq!(palette_card_width(80), 80);
    }

    #[test]
    fn description_wraps_when_narrow() {
        let desc = "Reload extensions and prompt templates from disk";
        let lines = wrap_palette_description(desc, 40);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn visible_terminal_rows_respects_viewport_cap() {
        let options = vec![
            elph_tui::SelectOption::new("/a", "First command with a longer description"),
            elph_tui::SelectOption::new("/b", "Second command with another longer description"),
        ];
        let rows = visible_terminal_rows(&options, 0, 2, 50, 3);
        assert_eq!(rows, 3);
    }
}
