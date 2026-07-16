//! Row metrics for slash palette command + wrapped description columns.

use elph_tui::types::SelectOption;
use elph_tui::utils::wrap_text;

use crate::types::SlashCommand;

/// Selection marker width (`❯ ` or `  `).
pub const ROW_PREFIX_CHARS: usize = 2;

/// Minimum gap between the command column and description column.
pub const CMD_DESC_GAP_COLS: u16 = 2;

/// Fallback command column width when the list is empty.
pub const CMD_COLUMN_MIN_CHARS: usize = 14;

/// Upper bound so long skill names do not consume the whole row.
pub const CMD_COLUMN_MAX_CHARS: usize = 44;

/// Minimum description column width after reserving the command column.
pub const MIN_DESC_COLUMN_CHARS: u16 = 12;

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

/// Width of one rendered command label (`❯ /name` or `  /name`).
pub fn palette_command_label_width(name: &str) -> usize {
    ROW_PREFIX_CHARS.saturating_add(name.chars().count())
}

/// Width of a command row including an optional dimmed args hint (` /tools [json|list]`).
pub fn palette_slash_row_label_width(command_name: &str, args_hint: Option<&str>) -> usize {
    let mut width = palette_command_label_width(command_name);
    if let Some(hint) = args_hint {
        width = width.saturating_add(1).saturating_add(hint.chars().count());
    }
    width
}

/// Command column width derived from the widest visible command name.
pub fn palette_command_column_width(options: &[SelectOption], list_width: u16) -> u16 {
    let mut max_label = CMD_COLUMN_MIN_CHARS;
    for option in options {
        max_label = max_label.max(palette_command_label_width(&option.name));
    }
    max_label = max_label.min(CMD_COLUMN_MAX_CHARS);

    let max_allowed = list_width
        .saturating_sub(CMD_DESC_GAP_COLS + MIN_DESC_COLUMN_CHARS)
        .max(1) as usize;
    max_label.min(max_allowed).max(1) as u16
}

/// Command column width when args hints render in a separate dimmed segment.
pub fn palette_command_column_width_for_commands(commands: &[SlashCommand], list_width: u16) -> u16 {
    let mut max_label = CMD_COLUMN_MIN_CHARS;
    for command in commands {
        let width = palette_slash_row_label_width(&command.palette_command_name(), command.args_hint.as_deref());
        max_label = max_label.max(width);
    }
    max_label = max_label.min(CMD_COLUMN_MAX_CHARS);

    let max_allowed = list_width
        .saturating_sub(CMD_DESC_GAP_COLS + MIN_DESC_COLUMN_CHARS)
        .max(1) as usize;
    max_label.min(max_allowed).max(1) as u16
}

/// Description column width in terminal cells.
pub fn palette_desc_width(list_width: u16, command_column_width: u16) -> usize {
    list_width
        .saturating_sub(command_column_width + CMD_DESC_GAP_COLS)
        .max(1) as usize
}

/// Wrapped description lines for one palette row (capped).
pub fn wrap_palette_description(description: &str, list_width: u16, command_column_width: u16) -> Vec<String> {
    let width = palette_desc_width(list_width, command_column_width);
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
pub fn palette_item_row_lines(description: &str, list_width: u16, command_column_width: u16) -> u16 {
    wrap_palette_description(description, list_width, command_column_width)
        .len()
        .max(1) as u16
}

/// Sum of terminal rows for a slice of options (capped at `viewport_cap`).
pub fn visible_terminal_rows(
    options: &[SelectOption],
    window_start: usize,
    item_cap: usize,
    list_width: u16,
    command_column_width: u16,
    viewport_cap: usize,
) -> u16 {
    let mut total = 0usize;
    for opt in options.iter().skip(window_start).take(item_cap) {
        total += palette_item_row_lines(&opt.description, list_width, command_column_width) as usize;
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
    fn command_column_grows_with_longest_name() {
        let options = vec![
            SelectOption::new("/goal", "Goals"),
            SelectOption::new("/skill:rust-verify-harden", "Audit Rust changes"),
        ];
        let width = palette_command_column_width(&options, 77);
        assert!(width >= palette_command_label_width("/skill:rust-verify-harden") as u16);
        assert!(width < 77);
    }

    #[test]
    fn command_column_respects_description_minimum() {
        let options = vec![SelectOption::new(
            "/skill:rust-verify-harden-with-extra-suffix",
            "Audit Rust changes",
        )];
        let list_width = 40u16;
        let cmd_col = palette_command_column_width(&options, list_width);
        let desc_col = palette_desc_width(list_width, cmd_col) as u16;
        assert!(desc_col >= MIN_DESC_COLUMN_CHARS);
    }

    #[test]
    fn description_wraps_when_narrow() {
        let desc = "Reload extensions and prompt templates from disk";
        let cmd_col = palette_command_column_width(&[], 40);
        let lines = wrap_palette_description(desc, 40, cmd_col);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn visible_terminal_rows_respects_viewport_cap() {
        let options = vec![
            SelectOption::new("/a", "First command with a longer description"),
            SelectOption::new("/b", "Second command with another longer description"),
        ];
        let cmd_col = palette_command_column_width(&options, 50);
        let rows = visible_terminal_rows(&options, 0, 2, 50, cmd_col, 3);
        assert_eq!(rows, 3);
    }
}
