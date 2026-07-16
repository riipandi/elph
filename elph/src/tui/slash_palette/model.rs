//! Pure slash-palette derivations from draft text and command registry.

use elph_tui::types::SelectOption;

use crate::agent::SlashArgCompletion;
use crate::agent::slash_arg_completions;
use crate::types::SlashCommand;

pub use super::fuzzy::filter_commands;

pub const MAX_VISIBLE_ROWS: u16 = 8;
pub const FAST_SCROLL_STEP: usize = 5;

/// Palette mode: filter command names, or complete command arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashPalettePhase {
    Command,
    Args { command: String },
}

/// Parsed slash draft for palette routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashDraftParts {
    pub command_query: String,
    pub args_command: Option<String>,
    pub args_query: String,
}

/// Render-ready snapshot derived from the editor draft and command list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashPaletteSnapshot {
    pub visible: bool,
    pub phase: SlashPalettePhase,
    pub query: String,
    pub filtered_commands: Vec<SlashCommand>,
    pub options: Vec<SelectOption>,
    pub list_height: u16,
    pub match_count: usize,
}

impl Default for SlashPaletteSnapshot {
    fn default() -> Self {
        Self::hidden()
    }
}

impl SlashPaletteSnapshot {
    pub fn hidden() -> Self {
        Self {
            visible: false,
            phase: SlashPalettePhase::Command,
            query: String::new(),
            filtered_commands: Vec::new(),
            options: Vec::new(),
            list_height: 0,
            match_count: 0,
        }
    }

    pub fn is_args_phase(&self) -> bool {
        matches!(self.phase, SlashPalettePhase::Args { .. })
    }

    pub fn should_render(&self) -> bool {
        self.visible
    }

    pub fn has_matches(&self) -> bool {
        !self.options.is_empty()
    }
}

pub fn parse_slash_draft(draft: &str) -> Option<SlashDraftParts> {
    let trimmed = draft.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    let body = trimmed.trim_start_matches('/').trim_start();
    if let Some((command, rest)) = body.split_once(' ') {
        let (args_query, _) = rest.split_once(' ').map_or((rest, ""), |(token, tail)| (token, tail));
        Some(SlashDraftParts {
            command_query: command.to_ascii_lowercase(),
            args_command: Some(command.to_ascii_lowercase()),
            args_query: args_query.trim().to_ascii_lowercase(),
        })
    } else {
        Some(SlashDraftParts {
            command_query: body.to_ascii_lowercase(),
            args_command: None,
            args_query: String::new(),
        })
    }
}

/// Palette is shown while typing a command name or args with known completions.
pub fn palette_visible(draft: &str) -> bool {
    let Some(parts) = parse_slash_draft(draft) else {
        return false;
    };
    if parts.args_command.is_none() {
        return true;
    }
    let command = parts.args_command.as_deref().unwrap_or("");
    slash_arg_completions(command).is_some()
}

pub fn query_from_draft(draft: &str) -> Option<String> {
    let parts = parse_slash_draft(draft)?;
    if parts.args_command.is_some() {
        Some(parts.args_query)
    } else {
        Some(parts.command_query)
    }
}

pub fn palette_phase_from_draft(draft: &str) -> Option<SlashPalettePhase> {
    let parts = parse_slash_draft(draft)?;
    if let Some(command) = parts.args_command.filter(|name| slash_arg_completions(name).is_some()) {
        Some(SlashPalettePhase::Args { command })
    } else {
        Some(SlashPalettePhase::Command)
    }
}

pub fn commands_to_options(commands: &[SlashCommand]) -> Vec<SelectOption> {
    commands
        .iter()
        .map(|cmd| SelectOption::new(cmd.palette_command_name(), cmd.description.clone()))
        .collect()
}

pub fn arg_completions_to_options(completions: &[SlashArgCompletion]) -> Vec<SelectOption> {
    completions
        .iter()
        .map(|entry| SelectOption::new(entry.value, entry.description))
        .collect()
}

pub fn filter_arg_completions(command: &str, query: &str) -> Vec<SlashArgCompletion> {
    let Some(all) = slash_arg_completions(command) else {
        return Vec::new();
    };
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return all.to_vec();
    }
    all.iter()
        .copied()
        .filter(|entry| entry.value.starts_with(&query))
        .collect()
}

pub fn list_viewport_cap(screen_height: u16) -> usize {
    if screen_height < 24 {
        4
    } else if screen_height < 36 {
        6
    } else {
        MAX_VISIBLE_ROWS as usize
    }
}

/// List body height: all matches when few, capped at viewport when many.
pub fn list_height(option_count: usize, screen_height: u16) -> u16 {
    if option_count == 0 {
        return 1;
    }
    option_count.min(list_viewport_cap(screen_height)).max(1) as u16
}

pub fn palette_window_start(selected: usize, height: usize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let viewport = height.max(1).min(len);
    let max_start = len.saturating_sub(viewport);
    selected.saturating_sub(viewport / 2).min(max_start)
}

pub fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

/// Slash input to dispatch when Enter confirms an overlay command in the palette.
pub fn palette_submit_slash_input(draft: &str, command_name: &str) -> String {
    complete_command(draft, command_name).trim_end().to_string()
}

pub fn complete_command(draft: &str, command_name: &str) -> String {
    let trimmed = draft.trim_start();
    let body = trimmed.trim_start_matches('/').trim_start();
    let args = body.split_once(' ').map(|(_, args)| args.trim()).unwrap_or("");
    if args.is_empty() {
        format!("/{command_name} ")
    } else {
        format!("/{command_name} {args}")
    }
}

pub fn complete_slash_arg(draft: &str, command_name: &str, arg_value: &str) -> String {
    let trimmed = draft.trim_start();
    let body = trimmed.trim_start_matches('/').trim_start();
    let tail = body
        .split_once(' ')
        .and_then(|(_, rest)| rest.split_once(' '))
        .map(|(_, rest)| rest.trim())
        .filter(|rest| !rest.is_empty());
    if let Some(rest) = tail {
        format!("/{command_name} {arg_value} {rest}")
    } else {
        format!("/{command_name} {arg_value} ")
    }
}

pub fn selected_command_name(filtered: &[SlashCommand], index: usize) -> Option<&str> {
    filtered
        .get(clamp_index(index, filtered.len()))
        .map(|cmd| cmd.name.as_str())
}

pub fn selected_arg_value(options: &[SelectOption], index: usize) -> Option<&str> {
    options
        .get(clamp_index(index, options.len()))
        .map(|opt| opt.name.as_str())
}

pub fn build_snapshot(draft: &str, commands: &[SlashCommand], screen_height: u16) -> SlashPaletteSnapshot {
    if !palette_visible(draft) {
        return SlashPaletteSnapshot::hidden();
    }
    let query = query_from_draft(draft).unwrap_or_default();
    let phase = palette_phase_from_draft(draft).unwrap_or(SlashPalettePhase::Command);
    let (filtered_commands, options) = match &phase {
        SlashPalettePhase::Command => {
            let filtered_commands = filter_commands(commands, &query);
            let options = commands_to_options(&filtered_commands);
            (filtered_commands, options)
        }
        SlashPalettePhase::Args { command } => {
            let filtered = filter_arg_completions(command, &query);
            let options = arg_completions_to_options(&filtered);
            (Vec::new(), options)
        }
    };
    let match_count = options.len();
    let list_height = list_height(match_count, screen_height);
    SlashPaletteSnapshot {
        visible: true,
        phase,
        query,
        filtered_commands,
        options,
        list_height,
        match_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_commands() -> Vec<SlashCommand> {
        vec![
            SlashCommand::new("compact", "Compact history"),
            SlashCommand::new("goal", "Manage goals"),
            SlashCommand::new("model", "Select model"),
        ]
    }

    #[test]
    fn palette_visible_only_for_slash_prefix() {
        assert!(!palette_visible("hello"));
        assert!(palette_visible("/model"));
        assert!(palette_visible("  /go"));
        assert!(palette_visible("/goal pause"));
        assert!(palette_visible("/tools j"));
        assert!(!palette_visible("/help args"));
        assert!(!palette_visible("/model filter"));
    }

    #[test]
    fn args_phase_lists_tools_formats() {
        let mut commands = sample_commands();
        commands.push(SlashCommand::new("tools", "Show active tools").with_args_hint("[json|list|table]"));
        let snapshot = build_snapshot("/tools j", &commands, 40);
        assert!(snapshot.is_args_phase());
        assert_eq!(snapshot.match_count, 1);
        assert_eq!(snapshot.options[0].name, "json");
    }

    #[test]
    fn complete_slash_arg_preserves_trailing_tokens() {
        assert_eq!(complete_slash_arg("/tools j extra", "tools", "json"), "/tools json extra");
        assert_eq!(complete_slash_arg("/tools j", "tools", "json"), "/tools json ");
    }

    #[test]
    fn list_height_fits_match_count_up_to_viewport_cap() {
        assert_eq!(list_height(3, 40), 3);
        assert_eq!(list_height(10, 40), 8);
        assert_eq!(list_height(0, 40), 1);
    }

    #[test]
    fn query_tracks_command_or_arg_token() {
        assert_eq!(query_from_draft("/goal pause").as_deref(), Some("pause"));
        assert_eq!(query_from_draft("/mod").as_deref(), Some("mod"));
        assert_eq!(query_from_draft("/tools ").as_deref(), Some(""));
    }

    #[test]
    fn filter_fuzzy_matches_prefix_case_insensitive() {
        let commands = sample_commands();
        let filtered = filter_commands(&commands, "GO");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "goal");
    }

    #[test]
    fn filter_fuzzy_matches_subsequence_in_name() {
        let commands = sample_commands();
        let filtered = filter_commands(&commands, "mdl");
        assert_eq!(filtered.first().map(|cmd| cmd.name.as_str()), Some("model"));
    }

    #[test]
    fn complete_replaces_command_and_preserves_args() {
        assert_eq!(complete_command("/mod", "model"), "/model ");
        assert_eq!(complete_command("/go pause", "goal"), "/goal pause");
    }

    #[test]
    fn snapshot_hidden_for_non_slash_draft() {
        let snapshot = build_snapshot("hello", &sample_commands(), 40);
        assert!(!snapshot.should_render());
    }

    #[test]
    fn snapshot_visible_even_without_matches() {
        let snapshot = build_snapshot("/zzz", &sample_commands(), 40);
        assert!(snapshot.should_render());
        assert!(!snapshot.has_matches());
        assert_eq!(snapshot.list_height, 1);
    }

    #[test]
    fn palette_window_start_clamps_at_list_tail() {
        let cap = 8;
        assert_eq!(palette_window_start(9, cap, 10), 2);
        assert_eq!(palette_window_start(0, cap, 3), 0);
        assert_eq!(palette_window_start(4, cap, 10), 0);
    }

    #[test]
    fn visible_item_window_stays_full_at_list_tail() {
        fn visible_item_window_size(selected: usize, option_count: usize, viewport_cap: usize) -> usize {
            if option_count == 0 {
                return 1;
            }
            let cap = viewport_cap.max(1).min(option_count);
            let window_start = palette_window_start(selected, cap, option_count);
            option_count.saturating_sub(window_start).min(cap).max(1)
        }
        let cap = 8;
        assert_eq!(visible_item_window_size(9, 10, cap), 8);
        assert_eq!(visible_item_window_size(0, 3, cap), 3);
        assert_eq!(visible_item_window_size(4, 10, cap), 8);
    }
}
