//! Row metrics for slash palette command + single-line description truncation.

use elph_tui::types::SelectOption;
use elph_tui::utils::{display_width, truncate_with_ellipsis};

use crate::types::SlashCommand;

/// Selection marker width (`❯ ` or `  `).
pub const ROW_PREFIX_CHARS: usize = 2;

/// Minimum gap between the command column and description column.
pub const CMD_DESC_GAP_COLS: u16 = 2;

/// Fallback command column width when the list is empty.
pub const CMD_COLUMN_MIN_CHARS: usize = 14;

/// Minimum description column width after reserving the command column.
pub const MIN_DESC_COLUMN_CHARS: u16 = 12;

/// Outer card width — matches the editor chrome (`screen_width`).
pub fn palette_card_width(screen_width: u16) -> u16 {
    screen_width.max(20)
}

/// List content width inside the card frame (editor inner width minus scrollbar column).
pub fn palette_list_width(screen_width: u16) -> u16 {
    screen_width.saturating_sub(3).max(20)
}

/// Width of one rendered command label (`❯ /name` or `  /name`) in display columns.
pub fn palette_command_label_width(name: &str) -> usize {
    ROW_PREFIX_CHARS.saturating_add(display_width(name))
}

/// Width of a command row including an optional dimmed args hint (` /tools [json|list]`).
pub fn palette_slash_row_label_width(command_name: &str, args_hint: Option<&str>) -> usize {
    let mut width = palette_command_label_width(command_name);
    if let Some(hint) = args_hint {
        width = width.saturating_add(1).saturating_add(display_width(hint));
    }
    width
}

/// Command column width derived from the widest visible command name.
pub fn palette_command_column_width(options: &[SelectOption], list_width: u16) -> u16 {
    let mut max_label = CMD_COLUMN_MIN_CHARS;
    for option in options {
        max_label = max_label.max(palette_command_label_width(&option.name));
    }

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

/// Truncate command name (and optional args hint) to fit the command column content width.
///
/// `content_max` is the available display columns *after* the row prefix (`❯ `) has been
/// reserved. When the column is narrow (≤ 30 columns) a 1-column safety margin is reserved
/// to prevent rendering overlap; on wider columns the full content_max is usable so
/// truncation is driven purely by actual space.
///
/// Returns `(display_name, display_hint)` — hint is `None` when not provided, when there is
/// no room left after the name, or when it would be truncated to fewer than 3 columns
/// (rendering it useless).
pub fn truncate_command_label(
    command_name: &str,
    args_hint: Option<&str>,
    content_max: usize,
) -> (String, Option<String>) {
    if content_max < 2 {
        if content_max == 1 {
            return ("…".to_string(), None);
        }
        return (String::new(), None);
    }
    // Safety margin only on narrow columns where overlap is likely.
    let budget = if content_max <= 30 {
        content_max.saturating_sub(1)
    } else {
        content_max
    };

    let Some(hint) = args_hint.filter(|h| !h.is_empty()) else {
        return (truncate_with_ellipsis(command_name, budget), None);
    };

    let hint_width = display_width(hint);
    let name_width = display_width(command_name);

    // name + space + hint fits comfortably within budget.
    if name_width.saturating_add(1).saturating_add(hint_width) <= budget {
        return (command_name.to_string(), Some(hint.to_string()));
    }

    // Prefer keeping the full hint when the name can still show a useful prefix.
    let name_budget = budget.saturating_sub(1 + hint_width);
    if name_budget >= 4 {
        return (truncate_with_ellipsis(command_name, name_budget), Some(hint.to_string()));
    }

    // Narrow column: share remaining space between name and hint.
    let name_budget = (budget * 2 / 3).max(1);
    let display_name = truncate_with_ellipsis(command_name, name_budget);
    let used = display_width(&display_name).saturating_add(1);
    let hint_budget = budget.saturating_sub(used);

    // Drop hint when it would be truncated to fewer than 3 columns — e.g. "[…"
    // or "[j…" — which is visually noisy without conveying useful information.
    if hint_budget < 3 {
        (display_name, None)
    } else {
        (display_name, Some(truncate_with_ellipsis(hint, hint_budget)))
    }
}

/// Truncate description to a single line fitting the description column width.
///
/// Always returns a single-element vec so callers that expect `Vec<String>` continue to work.
pub fn wrap_palette_description(description: &str, list_width: u16, command_column_width: u16) -> Vec<String> {
    let width = palette_desc_width(list_width, command_column_width);
    vec![truncate_with_ellipsis(description, width)]
}

/// Sum of terminal rows for a slice of options (capped at `viewport_cap`).
///
/// Each item occupies exactly one row (descriptions are truncated, not wrapped).
pub fn visible_terminal_rows(
    options: &[SelectOption],
    window_start: usize,
    item_cap: usize,
    _list_width: u16,
    _command_column_width: u16,
    viewport_cap: usize,
) -> u16 {
    let available = options.len().saturating_sub(window_start);
    available.min(item_cap).min(viewport_cap).max(1) as u16
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
            SelectOption::new("/rust-verify-harden", "Audit Rust changes"),
        ];
        let width = palette_command_column_width(&options, 77);
        assert!(width >= palette_command_label_width("/rust-verify-harden") as u16);
        assert!(width < 77);
    }

    #[test]
    fn command_column_respects_description_minimum() {
        let options = vec![SelectOption::new(
            "/rust-verify-harden-with-extra-suffix",
            "Audit Rust changes",
        )];
        let list_width = 40u16;
        let cmd_col = palette_command_column_width(&options, list_width);
        let desc_col = palette_desc_width(list_width, cmd_col) as u16;
        assert!(desc_col >= MIN_DESC_COLUMN_CHARS);
    }

    #[test]
    fn description_truncated_to_single_line() {
        let desc = "Reload extensions and prompt templates from disk";
        let cmd_col = palette_command_column_width(&[], 40);
        let lines = wrap_palette_description(desc, 40, cmd_col);
        assert_eq!(lines.len(), 1, "description is truncated to a single line");
        assert!(
            display_width(&lines[0]) <= palette_desc_width(40, cmd_col),
            "truncated description fits within desc_width"
        );
    }

    #[test]
    fn visible_terminal_rows_respects_viewport_cap() {
        let options = vec![
            SelectOption::new("/a", "First command with a longer description"),
            SelectOption::new("/b", "Second command with another longer description"),
        ];
        let cmd_col = palette_command_column_width(&options, 50);
        // Each item is 1 row; with 2 items and viewport_cap=3, the cap doesn't trigger.
        let rows = visible_terminal_rows(&options, 0, 2, 50, cmd_col, 3);
        assert_eq!(rows, 2);
        // With viewport_cap=1, only 1 row fits.
        let rows = visible_terminal_rows(&options, 0, 2, 50, cmd_col, 1);
        assert_eq!(rows, 1);
    }

    #[test]
    fn truncate_command_label_ellipsizes_long_names() {
        let (name, hint) = truncate_command_label("/skill:very-long-skill-name-here", None, 18);
        assert_eq!(hint, None);
        assert!(name.ends_with('…'));
        assert!(
            display_width(&name) <= 18,
            "name display width {} exceeds {}",
            display_width(&name),
            18
        );
    }

    #[test]
    fn truncate_command_label_keeps_args_hint_when_possible() {
        let (name, hint) = truncate_command_label("/tools", Some("[json|list|table]"), 40);
        assert_eq!(name, "/tools");
        assert_eq!(hint.as_deref(), Some("[json|list|table]"));
    }

    #[test]
    fn truncate_command_label_drops_hint_when_too_narrow() {
        // content_max=8 → budget=7 → "/tools" (6) + 1 = 7, hint_budget = 0 < 3 → hint dropped.
        let (name, hint) = truncate_command_label("/tools", Some("[json|list|table]"), 8);
        assert!(hint.is_none(), "hint should be dropped when very narrow");
        assert!(display_width(&name) <= 7);
    }

    #[test]
    fn truncate_command_label_shares_space_on_narrow_column() {
        // content_max=14 → budget=13 → name_budget = 13-1-5=7 ≥ 4, keeps full hint.
        let (name, hint) = truncate_command_label("/copy", Some("[id]"), 14);
        assert_eq!(hint.as_deref(), Some("[id]"), "full hint kept when name budget >= 4");
        assert!(display_width(&name) <= 7);
    }

    #[test]
    fn truncate_command_label_shares_space_when_tight() {
        // content_max=16 → budget=15 → name_budget = 15-1-12=2 (<4) → shares space.
        let (name, hint) = truncate_command_label("/tools", Some("[json|list]"), 16);
        assert!(name.starts_with("/"));
        assert!(name.ends_with('…') || display_width(&name) == display_width("/tools"));
        // Hint should be truncated but still present (budget ≥ 3).
        assert!(hint.is_some(), "hint shown when hint_budget >= 3");
        if let Some(ref h) = hint {
            assert!(display_width(h) >= 3, "truncated hint should be at least 3 columns wide");
        }
    }
}
