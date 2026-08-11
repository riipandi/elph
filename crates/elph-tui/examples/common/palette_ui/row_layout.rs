//! Row metrics for slash palette command + wrapped description columns.

use elph_tui::types::SelectOption;
use elph_tui::utils::wrap_text;

pub const ROW_PREFIX_CHARS: usize = 2;
pub const CMD_DESC_GAP_COLS: u16 = 2;
pub const CMD_COLUMN_MIN_CHARS: usize = 14;
pub const CMD_COLUMN_MAX_CHARS: usize = 44;
pub const MIN_DESC_COLUMN_CHARS: u16 = 12;
pub const MAX_DESC_WRAP_LINES: usize = 3;

pub fn palette_card_width(screen_width: u16) -> u16 {
    screen_width.max(20)
}

pub fn palette_list_width(screen_width: u16) -> u16 {
    // Card round border (2 columns) plus side paddings (1 each) — rows stop one column
    // short of the right border so descriptions keep breathing room.
    screen_width.saturating_sub(4).max(20)
}

pub fn palette_command_label_width(name: &str) -> usize {
    ROW_PREFIX_CHARS.saturating_add(name.chars().count())
}

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

pub fn palette_desc_width(list_width: u16, command_column_width: u16) -> usize {
    list_width
        .saturating_sub(command_column_width + CMD_DESC_GAP_COLS)
        .max(1) as usize
}

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

pub fn palette_item_row_lines(description: &str, list_width: u16, command_column_width: u16) -> u16 {
    wrap_palette_description(description, list_width, command_column_width)
        .len()
        .max(1) as u16
}

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
