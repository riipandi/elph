//! Fuzzy command palette for tuie slash commands.

use crate::diff::{SlashCommand, fuzzy_filter};
use crate::theme::Theme;
use tuie::prelude::*;

struct PaletteRenderCtx {
    items: Vec<SlashCommand>,
    selected: usize,
    theme: Theme,
}

/// Returns true when the palette should track input (`/cmd` without a space).
pub fn palette_visible(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with('/') && !trimmed.contains(' ')
}

fn filtered_commands(commands: &[SlashCommand], input: &str, forced: bool) -> Vec<SlashCommand> {
    if forced {
        let query = input.trim();
        if query.is_empty() {
            return commands.to_vec();
        }
        return fuzzy_filter(commands, query, |cmd| cmd.name.clone());
    }
    let query = input.trim_start_matches('/').trim_start();
    if query.is_empty() {
        return commands.to_vec();
    }
    fuzzy_filter(commands, query, |cmd| cmd.name.clone())
}

/// Selection state for the fuzzy command palette popup.
#[derive(Debug, Clone, Default)]
pub struct CommandPaletteState {
    pub selected: usize,
    pub filter_key: String,
    /// When true (Ctrl+K), palette lists all commands without requiring a `/` prefix.
    pub forced: bool,
}

impl CommandPaletteState {
    pub fn sync_filter(&mut self, input: &str) {
        let query = input.trim_start_matches('/').trim_start();
        if self.filter_key != query {
            self.filter_key = query.to_string();
            self.selected = 0;
        }
    }

    pub fn move_up(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = self.selected.saturating_sub(1).min(len - 1);
    }

    pub fn move_down(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        if self.selected + 1 < len {
            self.selected += 1;
        }
    }

    pub fn selected_command(&self, commands: &[SlashCommand], input: &str) -> Option<SlashCommand> {
        let filtered = filtered_commands(commands, input, self.forced);
        filtered
            .get(self.selected.min(filtered.len().saturating_sub(1)))
            .cloned()
    }
}

fn render_palette_row(ctx: &mut PaletteRenderCtx, idx: usize) -> Option<Box<dyn Widget>> {
    let cmd = ctx.items.get(idx)?;
    let marker = if idx == ctx.selected { "› " } else { "  " };
    let row = format!("{marker}/{}  {}", cmd.name, cmd.description);
    let style = if idx == ctx.selected {
        Style::new().fg(ctx.theme.highlight())
    } else {
        Style::new().fg(ctx.theme.foreground)
    };
    Some(Text::new().content(row).style(style) as Box<dyn Widget>)
}

fn palette_render_ctx(filtered: Vec<SlashCommand>, selected: usize, theme: Theme) -> PaletteRenderCtx {
    PaletteRenderCtx {
        items: filtered,
        selected,
        theme,
    }
}

/// Returns the number of commands visible for the current filter.
pub fn filtered_command_count(commands: &[SlashCommand], input: &str, forced: bool) -> usize {
    filtered_commands(commands, input, forced).len()
}

/// Builds a bordered list popup for the active filter.
pub fn build_palette_widget(
    commands: &[SlashCommand],
    input: &str,
    state: &CommandPaletteState,
    theme: Theme,
) -> Box<List> {
    let filtered = filtered_commands(commands, input, state.forced);
    let selected = state.selected.min(filtered.len().saturating_sub(1));

    let mut list = List::new();
    list.set_renderer(palette_render_ctx(filtered, selected, theme), render_palette_row);
    list.set_item_count(filtered_command_count(commands, input, state.forced));
    list.bordered().border_style(Style::new().fg(theme.blue_col()))
}

/// Refreshes an open palette list without reopening the popup.
pub fn refresh_palette_list(
    list: &mut List,
    commands: &[SlashCommand],
    input: &str,
    state: &CommandPaletteState,
    theme: Theme,
) {
    let filtered = filtered_commands(commands, input, state.forced);
    let selected = state.selected.min(filtered.len().saturating_sub(1));
    let count = filtered.len();
    list.set_renderer(palette_render_ctx(filtered, selected, theme), render_palette_row);
    list.set_item_count(count);
    list.set_border_style(Style::new().fg(theme.blue_col()));
    list.dirty_layout();
}

/// Text inserted into the prompt when a palette row is accepted.
pub fn palette_selection_text(cmd: &SlashCommand) -> String {
    format!("/{}", cmd.name)
}

/// Opens the palette popup anchored above the prompt and returns its widget id.
pub fn open_palette_popup(widget: Box<List>) -> WidgetId<List> {
    let mut popup_id = WidgetId::EMPTY;
    let widget = widget.id(&mut popup_id);
    tuie::open_popup(
        Popup::new(widget)
            .placement(Placement::side(Direction2D::Up, Sign::Positive, Align::Start).offset(Vec2::new(0, -1)))
            .dismissible(),
    );
    popup_id
}

/// Closes the palette popup identified by `id`.
pub fn close_palette_popup(id: WidgetId<impl ?Sized>) {
    tuie::close_popup(id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::owly_builtin_commands;

    #[test]
    fn palette_visible_requires_slash_without_space() {
        assert!(palette_visible("/help"));
        assert!(palette_visible("  /init"));
        assert!(!palette_visible("/help args"));
        assert!(!palette_visible("hello"));
        assert!(!palette_visible(""));
    }

    #[test]
    fn forced_mode_filters_without_slash_prefix() {
        let commands = owly_builtin_commands();
        let state = CommandPaletteState {
            selected: 0,
            filter_key: String::new(),
            forced: true,
        };
        let all = state.selected_command(&commands, "").unwrap();
        assert_eq!(all.name, "help");

        let filtered = CommandPaletteState {
            selected: 0,
            filter_key: "ini".into(),
            forced: true,
        };
        let cmd = filtered.selected_command(&commands, "ini").unwrap();
        assert_eq!(cmd.name, "init");
    }

    #[test]
    fn slash_mode_uses_prefix_query() {
        let commands = owly_builtin_commands();
        let mut state = CommandPaletteState::default();
        state.sync_filter("/up");
        let cmd = state.selected_command(&commands, "/up").unwrap();
        assert_eq!(cmd.name, "update");
    }

    #[test]
    fn navigation_clamps_selection() {
        let mut state = CommandPaletteState::default();
        state.move_down(3);
        state.move_down(3);
        assert_eq!(state.selected, 2);
        state.move_up(3);
        assert_eq!(state.selected, 1);
        state.move_up(3);
        assert_eq!(state.selected, 0);
    }
}
